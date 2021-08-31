use core::time::Duration;

/// Compute the delay in accordance with the exponential backoff algorithm.
///
/// Uses the `distribution` and `attempts_so_far` values to compute the number of `slot_time`'s
/// that should be delayed for, and returns a new `Duration` with that value.
///
/// This is _not_ the "truncated" version.  If `attempts_so_far` is particularly large, the
/// resulting duration can be _very_ long.  For example, for `attempts_so_far == 20`, a maximum of
/// `2^20 - 1` times the `slot_time` would be taken.  Note that the practical limit of `i32::MAX`
/// is used as a stopping point.
///
/// It is recommended, if the algorithm should be biased towards retrying sooner, to use a
/// left-skewed distribution.
pub fn wait_size<D: rand::distributions::Distribution<f32>, R: rand::Rng>(
	slot_time: &Duration,
	attempts_so_far: u32,
	rng: &mut R,
	distribution: &D,
) -> Duration {
	wait_size_truncated(
		slot_time,
		attempts_so_far,
		i32::MAX as u32,
		rng,
		distribution,
	)
}

/// Compute the delay in accordance with the exponential backoff algorithm.
///
/// Uses the `distribution` and `attempts_so_far` values to compute the number of `slot_time`'s
/// that should be delayed for, and returns a new `Duration` with that value.
///
/// This is the "truncated" version of the exponential backoff computation function, and prevents
/// the exponentiation from continuing to a very large number.  A constant like `10` would prevent
/// more than `1023` instances of the `slot_time` from being used.
///
/// It is recommended, if the algorithm should be biased towards retrying sooner, to use a
/// left-skewed distribution.
pub fn wait_size_truncated<D: rand::distributions::Distribution<f32>, R: rand::Rng>(
	slot_time: &Duration,
	attempts_so_far: u32,
	exponent_max: u32,
	rng: &mut R,
	distribution: &D,
) -> Duration {
	let attempts_so_far: i32 = attempts_so_far.clamp(0_u32, exponent_max) as i32;
	let position = distribution.sample(rng);
	let max = 2_f32.powi(attempts_so_far) - 1.0_f32;
	slot_time.mul_f32(position * max)
}

#[cfg(test)]
mod tests {
	use super::*;

	mod test_distribution {
		pub(super) struct Always1 {}

		impl rand::distributions::Distribution<f32> for Always1 {
			fn sample<R: rand::Rng + ?Sized>(&self, _: &mut R) -> f32 {
				1.0
			}
		}
	}

	#[test]
	fn attempts_0_wait_size_0() {
		let distribution = test_distribution::Always1 {};
		let mut rng = rand::thread_rng();

		let slot_time = Duration::new(1, 0);

		assert_eq!(
			wait_size(&slot_time, 0_u32, &mut rng, &distribution),
			Duration::new(0, 0)
		);
	}

	#[test]
	fn attempts_1_wait_size_1() {
		let distribution = test_distribution::Always1 {};
		let mut rng = rand::thread_rng();

		let slot_time = Duration::new(1, 0);

		assert_eq!(
			wait_size(&slot_time, 1_u32, &mut rng, &distribution),
			Duration::new(1, 0)
		);
	}

	#[test]
	fn attempts_2_wait_size_3() {
		let distribution = test_distribution::Always1 {};
		let mut rng = rand::thread_rng();

		let slot_time = Duration::new(1, 0);

		assert_eq!(
			wait_size(&slot_time, 2_u32, &mut rng, &distribution),
			Duration::new(3, 0)
		);
	}

	#[test]
	fn attempts_3_wait_size_7() {
		let distribution = test_distribution::Always1 {};
		let mut rng = rand::thread_rng();

		let slot_time = Duration::new(1, 0);

		assert_eq!(
			wait_size(&slot_time, 3_u32, &mut rng, &distribution),
			Duration::new(7, 0)
		);
	}
}
