#![doc = include_str!("backoff.md")]

use rand::{Rng, RngExt};
use std::{num::NonZeroU32, time::Duration};

/// A completed failed attempt, passed to strategies and retry hooks.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Attempt {
	/// Attempt number, starting at one for the initial attempt.
	pub number: NonZeroU32,
	/// Runtime of this attempt, including any time awaiting its result.
	pub duration: Duration,
	/// Wall-clock time since the first attempt started, including sleeps.
	pub elapsed: Duration,
}

impl Attempt {
	/// Construct metadata for a completed failed attempt.
	#[must_use]
	pub const fn new(number: NonZeroU32, duration: Duration, elapsed: Duration) -> Self {
		Self {
			number,
			duration,
			elapsed,
		}
	}
}

/// A stateful schedule that decides whether and when to retry.
///
/// When used with [`Retry`](crate::Retry), `Clone` must produce independent
/// per-execution state. For example, deriving `Clone` over `Arc<Mutex<_>>` would
/// share history across executions, violating this requirement.
///
/// A [`Duration`] implements this trait as a constant delay that never stops.
/// ```
/// use eb::Retry;
/// use std::{num::NonZeroU32, time::Duration};
/// let result = Retry::new().with_backoff(Duration::from_millis(1))
///     .max_attempts(NonZeroU32::new(2).unwrap())
///     .retry_blocking(|| Err::<(), _>("again")).run();
/// assert_eq!(result, Err("again"));
/// ```
///
/// Custom strategies use [`eb::rand::Rng`](crate::rand::Rng). Rand 0.10 is a
/// public dependency, re-exported at the crate root.
/// ```
/// use eb::{backoff::{Attempt, Backoff}, rand::Rng, Retry};
/// use std::time::Duration;
/// #[derive(Clone)]
/// struct ThreeAttempts;
/// impl Backoff for ThreeAttempts {
///     fn next_delay(&mut self, failed: &Attempt, _: &mut dyn Rng) -> Option<Duration> {
///         (failed.number.get() < 3).then_some(Duration::ZERO)
///     }
/// }
/// assert_eq!(Retry::new().with_backoff(ThreeAttempts)
///     .retry_blocking(|| Err::<(), _>("again")).run(), Err("again"));
/// ```
pub trait Backoff {
	/// Return the delay after `failed`, or `None` to stop. The RNG is object-safe.
	fn next_delay(&mut self, failed: &Attempt, rng: &mut dyn Rng) -> Option<Duration>;
}

/// The duration of a single backoff slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Slot {
	/// A constant slot duration.
	Fixed(Duration),
	/// Arithmetic mean of failed attempt runtimes in this execution.
	Measured {
		/// Lower bound on the measured mean.
		min: Duration,
	},
}
impl Slot {
	/// Measure failed runtimes without a minimum floor.
	pub fn measured() -> Self {
		Self::Measured {
			min: Duration::ZERO,
		}
	}
}

/// Randomization applied to an exponential window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Jitter {
	/// Uniform continuous fraction in `[0, 1)` of the window (nanosecond rounded).
	#[default]
	Continuous,
	/// Uniform integer number of slots, including both window endpoints.
	Discrete,
}

/// Binary exponential backoff with bounded exponents and saturating durations.
///
/// After failure `n`, the window is `slot * (2^min(n, exponent_cap) - 1)`.
/// Defaults: fixed one-second slots, continuous jitter, exponent cap ten, no cap
/// on delay. The delay cap is applied after jitter.
#[derive(Clone, Debug)]
#[must_use]
pub struct BinaryExponential {
	slot: Slot,
	exponent_cap: u32,
	jitter: Jitter,
	max_delay: Option<Duration>,
	mean: f64,
	samples: f64,
}
impl Default for BinaryExponential {
	fn default() -> Self {
		Self::new(Slot::Fixed(Duration::from_secs(1)))
	}
}
impl BinaryExponential {
	/// Create a strategy with the given slot and otherwise default settings.
	pub fn new(slot: Slot) -> Self {
		Self {
			slot,
			exponent_cap: 10,
			jitter: Jitter::Continuous,
			max_delay: None,
			mean: 0.0,
			samples: 0.0,
		}
	}
	/// Replace the slot configuration, clearing any measured history.
	pub fn slot(mut self, slot: Slot) -> Self {
		self.slot = slot;
		self.mean = 0.0;
		self.samples = 0.0;
		self
	}
	/// Set the exponent cap, clamped to 52 so `2^k - 1` is exact in `f64`.
	pub fn exponent_cap(mut self, cap: u32) -> Self {
		self.exponent_cap = cap.min(52);
		self
	}
	/// Choose continuous or discrete jitter.
	pub fn jitter(mut self, jitter: Jitter) -> Self {
		self.jitter = jitter;
		self
	}
	/// Set a hard upper bound applied after jitter.
	pub fn max_delay(mut self, delay: Duration) -> Self {
		self.max_delay = Some(delay);
		self
	}
	/// IEEE 802.3 style fixed slots, discrete jitter, and exponent cap ten.
	///
	/// Ethernet also gives up after sixteen total attempts; pair this preset with
	/// the driver's limit:
	/// ```
	/// use eb::{Retry, backoff::BinaryExponential};
	/// use std::{num::NonZeroU32, time::Duration};
	/// let policy = Retry::new()
	///     .with_backoff(BinaryExponential::ieee_802_3(Duration::from_micros(51)))
	///     .max_attempts(NonZeroU32::new(16).unwrap());
	/// assert_eq!(policy.retry_blocking(|| Ok::<_, ()>(())).run(), Ok(()));
	/// ```
	pub fn ieee_802_3(slot: Duration) -> Self {
		Self::new(Slot::Fixed(slot)).jitter(Jitter::Discrete)
	}
}
impl Backoff for BinaryExponential {
	// Both integer casts are exact because the exponent is clamped to 52.
	#[allow(clippy::cast_precision_loss)]
	fn next_delay(&mut self, failed: &Attempt, rng: &mut dyn Rng) -> Option<Duration> {
		let slot = match self.slot {
			Slot::Fixed(slot) => slot.as_secs_f64(),
			Slot::Measured { min } => {
				self.samples += 1.0;
				self.mean += (failed.duration.as_secs_f64() - self.mean) / self.samples;
				self.mean.max(min.as_secs_f64())
			}
		};
		let k = failed.number.get().min(self.exponent_cap);
		let slots = (1_u64 << k) - 1;
		let multiplier = match self.jitter {
			Jitter::Continuous => rng.random::<f64>() * slots as f64,
			Jitter::Discrete => rng.random_range(0..=slots) as f64,
		};
		let cap = self.max_delay.unwrap_or(Duration::MAX);
		let seconds = slot * multiplier;
		Some(if seconds >= cap.as_secs_f64() {
			cap
		} else {
			Duration::try_from_secs_f64(seconds).unwrap_or(cap).min(cap)
		})
	}
}

/// Constant delay strategy; never stops on its own.
impl Backoff for Duration {
	fn next_delay(&mut self, _: &Attempt, _: &mut dyn Rng) -> Option<Duration> {
		Some(*self)
	}
}

/// Strategy adapter measuring delays from the failed attempt's start.
#[derive(Clone, Debug)]
pub struct BetweenStarts<B> {
	inner: B,
	enabled: bool,
}
/// Combinators available on every sized backoff strategy.
pub trait BackoffExt: Backoff + Sized {
	/// Subtract the failed attempt's runtime from the inner delay, clamped to zero.
	fn between_starts(self) -> BetweenStarts<Self> {
		self.between_starts_if(true)
	}
	/// Apply between-starts scheduling only when `enabled` is true.
	fn between_starts_if(self, enabled: bool) -> BetweenStarts<Self> {
		BetweenStarts {
			inner: self,
			enabled,
		}
	}
}
impl<B: Backoff> BackoffExt for B {}
impl<B: Backoff> Backoff for BetweenStarts<B> {
	fn next_delay(&mut self, failed: &Attempt, rng: &mut dyn Rng) -> Option<Duration> {
		self.inner.next_delay(failed, rng).map(|d| {
			if self.enabled {
				d.saturating_sub(failed.duration)
			} else {
				d
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rand::{rngs::SmallRng, SeedableRng};
	fn attempt(number: u32, seconds: u64) -> Attempt {
		Attempt::new(
			NonZeroU32::new(number).unwrap(),
			Duration::from_secs(seconds),
			Duration::ZERO,
		)
	}
	#[test]
	fn continuous_matches_seeded_uniform_window() {
		let mut rng = SmallRng::seed_from_u64(42);
		let mut expected_rng = rng.clone();
		let mut b = BinaryExponential::new(Slot::Fixed(Duration::from_secs(2)));
		for n in 1..20 {
			let expected = Duration::from_secs_f64(
				expected_rng.random::<f64>() * 2.0 * f64::from((1_u32 << n.min(10)) - 1),
			);
			assert_eq!(b.next_delay(&attempt(n, 0), &mut rng), Some(expected));
		}
	}
	#[test]
	fn discrete_is_uniform_integer_slots_including_endpoints() {
		let mut rng = SmallRng::seed_from_u64(7);
		let mut expected_rng = rng.clone();
		let mut b = BinaryExponential::ieee_802_3(Duration::from_secs(2));
		let mut counts = [0; 4];
		for _ in 0..4000 {
			let slots = expected_rng.random_range(0_u64..=3);
			let delay = b.next_delay(&attempt(2, 0), &mut rng).unwrap();
			assert_eq!(delay, Duration::from_secs(slots * 2));
			counts[usize::try_from(slots).unwrap()] += 1;
		}
		assert!(counts.into_iter().all(|count| (850..1150).contains(&count)));
	}
	#[test]
	fn measured_average_and_floor() {
		let mut rng = SmallRng::seed_from_u64(3);
		let mut b = BinaryExponential::new(Slot::Measured {
			min: Duration::from_secs(3),
		});
		for (n, duration, expected_slot) in [(1, 2, 3.0), (2, 6, 4.0), (3, 1, 3.0)] {
			let mut reference = rng.clone();
			let multiplier = reference.random::<f64>() * f64::from((1_u32 << n) - 1);
			assert_eq!(
				b.next_delay(&attempt(n, duration), &mut rng),
				Some(Duration::from_secs_f64(expected_slot * multiplier))
			);
		}
	}
	#[test]
	fn between_starts_saturates() {
		let mut b = Duration::from_secs(3).between_starts();
		let mut rng = SmallRng::seed_from_u64(1);
		for (duration, delay) in [(1, 2), (3, 0), (9, 0)] {
			assert_eq!(
				b.next_delay(&attempt(1, duration), &mut rng),
				Some(Duration::from_secs(delay))
			);
		}
	}
	#[test]
	fn disabled_between_starts_preserves_delay() {
		let mut b = Duration::from_secs(3).between_starts_if(false);
		assert_eq!(
			b.next_delay(&attempt(1, 9), &mut SmallRng::seed_from_u64(1)),
			Some(Duration::from_secs(3))
		);
	}
	#[test]
	fn caps_and_extreme_slots_never_overflow() {
		let mut rng = SmallRng::seed_from_u64(1);
		for jitter in [Jitter::Continuous, Jitter::Discrete] {
			let mut b = BinaryExponential::new(Slot::Fixed(Duration::MAX))
				.exponent_cap(u32::MAX)
				.jitter(jitter);
			assert_eq!(b.exponent_cap, 52);
			assert_eq!(
				b.next_delay(&attempt(u32::MAX, 0), &mut rng),
				Some(Duration::MAX)
			);
			let mut b = b.max_delay(Duration::from_millis(5));
			assert_eq!(
				b.next_delay(&attempt(u32::MAX, 0), &mut rng),
				Some(Duration::from_millis(5))
			);
			let mut b = b.exponent_cap(0);
			assert_eq!(
				b.next_delay(&attempt(u32::MAX, 0), &mut rng),
				Some(Duration::ZERO)
			);
		}
	}
	#[test]
	fn hard_cap_is_after_jitter() {
		let mut rng = SmallRng::seed_from_u64(11);
		let mut reference = rng.clone();
		let mut b =
			BinaryExponential::new(Slot::Fixed(Duration::from_secs(4))).max_delay(Duration::from_secs(1));
		for _ in 0..100 {
			let raw = Duration::from_secs_f64(reference.random::<f64>() * 4.0);
			assert_eq!(
				b.next_delay(&attempt(1, 0), &mut rng),
				Some(raw.min(Duration::from_secs(1)))
			);
		}
	}
}
