//! Reusable policies and per-operation execution builders.
//!
//! A policy can be reused with different error types. Build an execution with
//! [`Retry::retry_blocking`] or [`Retry::retry_async`], optionally add a predicate or hook, then select a runner.
//!
//! ```
//! use eb::{Retry, Retryable};
//! use std::{future::ready, num::NonZeroU32, time::Duration};
//! # let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
//! # rt.block_on(async {
//! let policy = Retry::new().with_backoff(Duration::ZERO)
//!     .max_attempts(NonZeroU32::new(3).unwrap());
//! let mut calls = 0;
//! let value = (|| {
//!     calls += 1;
//!     ready(if calls < 3 { Err("transient") } else { Ok(42) })
//! }).retry_async(&policy).when(|e| *e == "transient")
//!   .run_async(|_| ready(())).await;
//! assert_eq!(value, Ok(42));
//! # });
//! ```
//!
//! Lending async closures use the policy directly:
//!
//! ```
//! use eb::Retry;
//! use std::{future::ready, time::Duration};
//! # let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
//! # rt.block_on(async {
//! let mut calls = 0;
//! let result = Retry::new().with_backoff(Duration::ZERO).retry_async_borrowing(async || {
//!     calls += 1;
//!     ready(()).await;
//!     if calls < 2 { Err("again") } else { Ok(calls) }
//! }).run_async_borrowing(|_| ready(())).await;
//! assert_eq!(result, Ok(2));
//! # });
//! ```
use crate::backoff::{Attempt, Backoff, BinaryExponential};
use rand::{rngs::SmallRng, Rng, SeedableRng};
use std::{
	future::Future,
	marker::PhantomData,
	num::NonZeroU32,
	time::{Duration, Instant},
};

/// Configuration independent of an operation's success and error types.
///
/// The first attempt is immediate. Counts include that attempt. There is no sleep
/// after success, rejected errors, or exhaustion. Each execution owns a clone of
/// the backoff, so measured history from executions cannot modify this policy.
/// For representable attempt metadata, execution stops with `AttemptsExhausted`
/// after `u32::MAX` failures even when the configured limit is unlimited.
#[derive(Clone, Debug)]
#[must_use]
pub struct Retry<B = BinaryExponential> {
	backoff: B,
	max_attempts: Option<NonZeroU32>,
	budget: Option<Duration>,
}
impl Default for Retry {
	fn default() -> Self {
		Self::new()
	}
}
impl Retry {
	/// Unlimited attempts with fixed one-second slots and continuous jitter.
	pub fn new() -> Self {
		Self {
			backoff: BinaryExponential::default(),
			max_attempts: None,
			budget: None,
		}
	}
}
impl<B> Retry<B> {
	/// Replace the strategy while preserving limits.
	pub fn with_backoff<B2>(self, backoff: B2) -> Retry<B2> {
		Retry {
			backoff,
			max_attempts: self.max_attempts,
			budget: self.budget,
		}
	}
	/// Limit total attempts, including the first.
	pub fn max_attempts(mut self, n: NonZeroU32) -> Self {
		self.max_attempts = Some(n);
		self
	}
	/// Remove the attempt limit, preserving any wall-clock budget.
	pub fn unlimited(mut self) -> Self {
		self.max_attempts = None;
		self
	}
	/// Limit wall-clock runtime, including operations and sleeps.
	///
	/// A retry is rejected if its next sleep would exceed the remaining budget.
	/// The budget is rechecked after the hook and sleep, before starting a retry.
	/// Running attempts are never interrupted; use your runtime's timeout for
	/// cancellation. The first attempt always runs, even with a zero budget.
	pub fn budget(mut self, total: Duration) -> Self {
		self.budget = Some(total);
		self
	}
}
impl<B: Backoff + Clone> Retry<B> {
	/// Bind success and error types from a blocking operation before configuring callbacks.
	/// ```
	/// use eb::{Retry, Retryable};
	/// use std::io;
	/// let policy = Retry::new();
	/// let op = || Err::<(), _>(io::Error::other("x"));
	/// let _ = policy.retry_blocking(op)
	///     .when(|e| e.kind() == io::ErrorKind::Interrupted)
	///     .on_retry(|e, info| { let _ = (e.kind(), info.delay); }).run();
	/// let _ = op.retry_blocking(&policy)
	///     .when(|e| e.kind() == io::ErrorKind::Interrupted)
	///     .on_retry(|e, info| { let _ = (e.kind(), info.delay); }).run();
	/// ```
	pub fn retry_blocking<T, E, F>(&self, op: F) -> Execution<B, F, E>
	where
		F: FnMut() -> Result<T, E>,
	{
		self.retry(op)
	}
	/// Bind success and error types from a fresh-future factory.
	/// ```
	/// use eb::{Retry, Retryable};
	/// use std::{future::ready, io};
	/// # let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
	/// # rt.block_on(async {
	/// let policy = Retry::new();
	/// let op = || ready(Err::<(), _>(io::Error::other("x")));
	/// let _ = policy.retry_async(op)
	///     .when(|e| e.kind() == io::ErrorKind::Interrupted)
	///     .on_retry(|e, info| { let _ = (e.kind(), info.delay); })
	///     .run_async(|_| ready(())).await;
	/// let _ = op.retry_async(&policy)
	///     .when(|e| e.kind() == io::ErrorKind::Interrupted)
	///     .on_retry(|e, info| { let _ = (e.kind(), info.delay); })
	///     .run_async(|_| ready(())).await;
	/// # });
	/// ```
	pub fn retry_async<T, E, F, Fut>(&self, op: F) -> Execution<B, F, E>
	where
		F: FnMut() -> Fut,
		Fut: Future<Output = Result<T, E>>,
	{
		self.retry(op)
	}
	/// Bind success and error types from a lending async closure.
	/// ```
	/// use eb::Retry;
	/// use std::{future::ready, io};
	/// # let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
	/// # rt.block_on(async {
	/// let mut calls = 0;
	/// let _ = Retry::new().retry_async_borrowing(async || {
	///     calls += 1;
	///     Err::<(), _>(io::Error::other("x"))
	/// }).when(|e| e.kind() == io::ErrorKind::Interrupted)
	///   .on_retry(|e, info| { let _ = (e.kind(), info.delay); })
	///   .run_async_borrowing(|_| ready(())).await;
	/// # });
	/// ```
	pub fn retry_async_borrowing<T, E, F>(&self, op: F) -> Execution<B, F, E>
	where
		F: AsyncFnMut() -> Result<T, E>,
	{
		self.retry(op)
	}

	/// Create an owned execution, independent of this policy's lifetime.
	/// The operation may be blocking, a future factory, or a lending async closure.
	pub fn retry<F, E>(&self, op: F) -> Execution<B, F, E> {
		Execution {
			policy: self.clone(),
			op,
			predicate: |_| true,
			hook: |_, _| {},
			rng: SmallRng::from_rng(&mut rand::rng()),
			mode: Plain,
			error: PhantomData,
		}
	}
}

/// Failure context supplied just before a retry sleep.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RetryInfo {
	/// The failed attempt that caused this retry.
	pub attempt: Attempt,
	/// Chosen delay, after all backoff adjustments.
	pub delay: Duration,
}
/// Why an execution returned its final error. Order of precedence is predicate,
/// attempt limit, backoff stop, then budget.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum StopReason {
	/// The retry predicate rejected the error.
	NotRetryable,
	/// The total attempt limit was reached.
	AttemptsExhausted,
	/// The wall-clock budget was reached or the next sleep would exceed it.
	BudgetExhausted,
	/// The strategy returned no further delay.
	BackoffStopped,
}
/// The last error and the circumstances in which retrying stopped.
#[derive(Debug)]
#[non_exhaustive]
pub struct RetryError<E> {
	/// Error returned by the final attempt.
	pub error: E,
	/// Total attempts, including the initial attempt.
	pub attempts: NonZeroU32,
	/// Wall-clock execution time, including sleeps.
	pub elapsed: Duration,
	/// Decision that stopped retries.
	pub reason: StopReason,
}
impl<E> RetryError<E> {
	/// Recover the operation's error, discarding retry diagnostics.
	pub fn into_inner(self) -> E {
		self.error
	}
}

impl<E: std::fmt::Display> std::fmt::Display for RetryError<E> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"retry stopped after {} attempts ({:?}): {}",
			self.attempts, self.reason, self.error
		)
	}
}
impl<E: std::error::Error + 'static> std::error::Error for RetryError<E> {
	fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
		Some(&self.error)
	}
}
/// Type-state selecting the final error representation.
#[doc(hidden)]
pub trait ErrorMode<E>: sealed::Sealed {
	/// Error returned by a terminal runner.
	type Error;
	/// Convert terminal failure information to the selected representation.
	fn finish(&self, failure: RetryError<E>) -> Self::Error;
}
/// Default mode returning the operation's original error unchanged.
#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
pub struct Plain;
/// Mode returning the operation's error with retry diagnostics.
#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
pub struct Detailed;
mod sealed {
	pub trait Sealed {}
	impl Sealed for super::Plain {}
	impl Sealed for super::Detailed {}
}
impl<E> ErrorMode<E> for Plain {
	type Error = E;
	fn finish(&self, f: RetryError<E>) -> E {
		f.error
	}
}
impl<E> ErrorMode<E> for Detailed {
	type Error = RetryError<E>;
	fn finish(&self, f: RetryError<E>) -> RetryError<E> {
		f
	}
}

/// Owned per-operation builder. Configure callbacks and RNG before running.
///
/// By default every error is retryable, no hook runs, and a freshly seeded
/// `SmallRng` is owned by the execution. The RNG is `Send`; no thread-local RNG
/// is held across an await. Dropping an async execution cancels further retries.
///
/// # Choosing a runner
///
/// - Blocking operation returning `Result`: [`run`](Self::run).
/// - Fresh-future factory: [`run_async`](Self::run_async), or `run_tokio`
///   with the `tokio` feature.
/// - Lending async closure: [`run_async_borrowing`](Self::run_async_borrowing),
///   or `run_tokio_borrowing` with the `tokio` feature.
#[must_use]
pub struct Execution<B, F, E, P = fn(&E) -> bool, H = fn(&E, &RetryInfo), R = SmallRng, M = Plain> {
	policy: Retry<B>,
	op: F,
	predicate: P,
	hook: H,
	rng: R,
	mode: M,
	error: PhantomData<fn(E)>,
}
impl<B, F, E, P, H, R, M> Execution<B, F, E, P, H, R, M> {
	/// Replace the retry predicate. `false` stops without sleeping or calling the hook.
	pub fn when<Q: FnMut(&E) -> bool>(self, predicate: Q) -> Execution<B, F, E, Q, H, R, M> {
		Execution {
			policy: self.policy,
			op: self.op,
			predicate,
			hook: self.hook,
			rng: self.rng,
			mode: self.mode,
			error: PhantomData,
		}
	}
	/// Replace the hook, called after initial retry checks and before sleeping.
	/// The budget is checked again after the hook; a slow hook can cancel the retry.
	pub fn on_retry<Q: FnMut(&E, &RetryInfo)>(self, hook: Q) -> Execution<B, F, E, P, Q, R, M> {
		Execution {
			policy: self.policy,
			op: self.op,
			predicate: self.predicate,
			hook,
			rng: self.rng,
			mode: self.mode,
			error: PhantomData,
		}
	}
	/// Inject an owned RNG, for example a seeded `SmallRng` for deterministic tests.
	pub fn rng<Q: Rng>(self, rng: Q) -> Execution<B, F, E, P, H, Q, M> {
		Execution {
			policy: self.policy,
			op: self.op,
			predicate: self.predicate,
			hook: self.hook,
			rng,
			mode: self.mode,
			error: PhantomData,
		}
	}
	/// Return [`RetryError`] diagnostics from terminal methods.
	///
	/// `RetryError<E>` implements `std::error::Error` only when `E` does.
	/// Operation error types such as `anyhow::Error` and `Box<dyn Error>` do not
	/// satisfy that bound, so detailed failures cannot be converted into those
	/// types directly with `?`. Recover the original error with
	/// `.map_err(RetryError::into_inner)?`, or inspect the public fields.
	/// ```
	/// use eb::{Retry, RetryError};
	/// use std::error::Error;
	/// fn request() -> Result<(), Box<dyn Error>> {
	///     Retry::new().retry_blocking(|| Ok::<_, Box<dyn Error>>(()))
	///         .detailed().run().map_err(RetryError::into_inner)?;
	///     Ok(())
	/// }
	/// request().unwrap();
	/// ```
	pub fn detailed(self) -> Execution<B, F, E, P, H, R, Detailed> {
		Execution {
			policy: self.policy,
			op: self.op,
			predicate: self.predicate,
			hook: self.hook,
			rng: self.rng,
			mode: Detailed,
			error: PhantomData,
		}
	}
}
#[derive(Debug)]
struct PendingRetry<E> {
	error: E,
	delay: Duration,
}
struct State {
	start: Instant,
	number: NonZeroU32,
}
impl State {
	fn new() -> Self {
		Self {
			start: Instant::now(),
			number: NonZeroU32::MIN,
		}
	}
}
impl<B: Backoff, F, E, P: FnMut(&E) -> bool, H: FnMut(&E, &RetryInfo), R: Rng, M: ErrorMode<E>>
	Execution<B, F, E, P, H, R, M>
{
	// The single synchronous decision machine shared by every terminal loop.
	fn failed(
		&mut self,
		state: &mut State,
		started: Instant,
		error: E,
	) -> Result<PendingRetry<E>, M::Error> {
		let attempt = Attempt::new(state.number, started.elapsed(), state.start.elapsed());
		let decision = if !(self.predicate)(&error) {
			Err(StopReason::NotRetryable)
		} else if self.policy.max_attempts.is_some_and(|n| state.number >= n)
			|| state.number.get() == u32::MAX
		{
			Err(StopReason::AttemptsExhausted)
		} else if let Some(delay) = self.policy.backoff.next_delay(&attempt, &mut self.rng) {
			let elapsed = state.start.elapsed();
			if self
				.policy
				.budget
				.is_some_and(|budget| elapsed >= budget || delay > budget.saturating_sub(elapsed))
			{
				Err(StopReason::BudgetExhausted)
			} else {
				Ok(delay)
			}
		} else {
			Err(StopReason::BackoffStopped)
		};
		match decision {
			Ok(delay) => {
				(self.hook)(&error, &RetryInfo { attempt, delay });
				self.check_budget(state, PendingRetry { error, delay }, delay)
			}
			Err(reason) => Err(self.mode.finish(RetryError {
				error,
				attempts: state.number,
				elapsed: state.start.elapsed(),
				reason,
			})),
		}
	}

	// Retain the error until the next operation can actually start.
	fn check_budget(
		&self,
		state: &State,
		pending: PendingRetry<E>,
		delay: Duration,
	) -> Result<PendingRetry<E>, M::Error> {
		let elapsed = state.start.elapsed();
		if self
			.policy
			.budget
			.is_some_and(|budget| elapsed >= budget || delay > budget.saturating_sub(elapsed))
		{
			Err(self.mode.finish(RetryError {
				error: pending.error,
				attempts: state.number,
				elapsed,
				reason: StopReason::BudgetExhausted,
			}))
		} else {
			Ok(pending)
		}
	}
	fn start_retry(&self, state: &mut State, pending: PendingRetry<E>) -> Result<(), M::Error> {
		self.check_budget(state, pending, Duration::ZERO)?;
		state.number = NonZeroU32::new(state.number.get() + 1).unwrap();
		Ok(())
	}

	/// Run blocking operations with `std::thread::sleep`.
	///
	/// # Errors
	/// Plain mode returns `Result<T, E>`; after `.detailed()`, returns
	/// `Result<T, RetryError<E>>` when retries stop.
	pub fn run<T>(self) -> Result<T, M::Error>
	where
		F: FnMut() -> Result<T, E>,
	{
		self.run_with_sleep(std::thread::sleep)
	}

	fn run_with_sleep<T>(mut self, mut sleep: impl FnMut(Duration)) -> Result<T, M::Error>
	where
		F: FnMut() -> Result<T, E>,
	{
		let mut state = State::new();
		loop {
			let started = Instant::now();
			match (self.op)() {
				Ok(value) => return Ok(value),
				Err(error) => {
					let pending = self.failed(&mut state, started, error)?;
					sleep(pending.delay);
					self.start_retry(&mut state, pending)?;
				}
			}
		}
	}

	/// Run fresh future factories with a caller-provided async sleeper. The returned
	/// future is `Send` when its captured state and operation/sleep futures are `Send`.
	/// See [Choosing a runner](Execution#choosing-a-runner) for lending async closures.
	///
	/// # Errors
	/// Plain mode returns `Result<T, E>`; after `.detailed()`, returns
	/// `Result<T, RetryError<E>>` when retries stop.
	pub async fn run_async<T, Fut, S, Sleep>(mut self, mut sleep: S) -> Result<T, M::Error>
	where
		F: FnMut() -> Fut,
		Fut: Future<Output = Result<T, E>>,
		S: FnMut(Duration) -> Sleep,
		Sleep: Future<Output = ()>,
	{
		let mut state = State::new();
		loop {
			let started = Instant::now();
			match (self.op)().await {
				Ok(value) => return Ok(value),
				Err(error) => {
					let pending = self.failed(&mut state, started, error)?;
					sleep(pending.delay).await;
					self.start_retry(&mut state, pending)?;
				}
			}
		}
	}

	/// Run lending async closures that may mutate captures across awaits.
	/// Use `run_async` for factories when a `Send` future is required.
	///
	/// # Errors
	/// Plain mode returns `Result<T, E>`; after `.detailed()`, returns
	/// `Result<T, RetryError<E>>` when retries stop.
	pub async fn run_async_borrowing<T, S, Sleep>(mut self, mut sleep: S) -> Result<T, M::Error>
	where
		F: AsyncFnMut() -> Result<T, E>,
		S: FnMut(Duration) -> Sleep,
		Sleep: Future<Output = ()>,
	{
		let mut state = State::new();
		loop {
			let started = Instant::now();
			match (self.op)().await {
				Ok(value) => return Ok(value),
				Err(error) => {
					let pending = self.failed(&mut state, started, error)?;
					sleep(pending.delay).await;
					self.start_retry(&mut state, pending)?;
				}
			}
		}
	}

	/// Run future factories using Tokio's timer.
	///
	/// # Panics
	/// Panics when a retry needs to sleep outside a time-enabled Tokio runtime.
	///
	/// # Errors
	/// Plain mode returns `Result<T, E>`; after `.detailed()`, returns
	/// `Result<T, RetryError<E>>` when retries stop.
	#[cfg(feature = "tokio")]
	pub async fn run_tokio<T, Fut>(self) -> Result<T, M::Error>
	where
		F: FnMut() -> Fut,
		Fut: Future<Output = Result<T, E>>,
	{
		self.run_async(tokio::time::sleep).await
	}
	/// Run lending async closures using Tokio's timer.
	///
	/// # Panics
	/// Panics when a retry needs to sleep outside a time-enabled Tokio runtime.
	///
	/// # Errors
	/// Plain mode returns `Result<T, E>`; after `.detailed()`, returns
	/// `Result<T, RetryError<E>>` when retries stop.
	#[cfg(feature = "tokio")]
	pub async fn run_tokio_borrowing<T>(self) -> Result<T, M::Error>
	where
		F: AsyncFnMut() -> Result<T, E>,
	{
		self.run_async_borrowing(tokio::time::sleep).await
	}
}
/// Closure sugar for [`Retry::retry`]. Lending async closures should use the
/// policy method directly, since they need not implement `FnMut() -> R`.
pub trait Retryable: Sized {
	/// Attach a blocking operation, inferring callback error types immediately.
	fn retry_blocking<B: Backoff + Clone, T, E>(self, policy: &Retry<B>) -> Execution<B, Self, E>
	where
		Self: FnMut() -> Result<T, E>,
	{
		policy.retry_blocking(self)
	}
	/// Attach a future factory, inferring callback error types immediately.
	fn retry_async<B: Backoff + Clone, T, E, Fut>(self, policy: &Retry<B>) -> Execution<B, Self, E>
	where
		Self: FnMut() -> Fut,
		Fut: Future<Output = Result<T, E>>,
	{
		policy.retry_async(self)
	}

	/// Attach this operation to a reusable policy.
	/// ```
	/// use eb::{Retry, Retryable};
	/// let policy = Retry::new();
	/// assert_eq!((|| Ok::<_, ()>(7)).retry_blocking(&policy).run(), Ok(7));
	/// ```
	fn retry<B: Backoff + Clone, E>(self, policy: &Retry<B>) -> Execution<B, Self, E> {
		policy.retry(self)
	}
}
impl<F: FnMut() -> R, R> Retryable for F {}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::backoff::Slot;
	use std::{cell::RefCell, future::ready};
	fn runtime() -> tokio::runtime::Runtime {
		tokio::runtime::Builder::new_current_thread()
			.enable_time()
			.build()
			.unwrap()
	}
	fn three() -> NonZeroU32 {
		NonZeroU32::new(3).unwrap()
	}
	fn policy() -> Retry<Duration> {
		Retry::new()
			.with_backoff(Duration::ZERO)
			.max_attempts(three())
	}
	#[test]
	fn policy_reused_with_different_errors_and_owned_execution() {
		let p = policy();
		assert_eq!(p.retry(|| Err::<(), _>(7)).run(), Err(7));
		assert_eq!(p.retry(|| Err::<(), _>("oops")).run(), Err("oops"));
		let execution = {
			let p = policy();
			p.retry(|| Ok::<_, ()>(42))
		};
		assert_eq!(execution.run(), Ok(42));
	}
	#[derive(Clone)]
	struct Stop;
	impl Backoff for Stop {
		fn next_delay(&mut self, _: &Attempt, _: &mut dyn Rng) -> Option<Duration> {
			None
		}
	}
	#[test]
	fn detailed_reasons_and_precedence() {
		let e = policy()
			.retry(|| Err::<(), _>(7))
			.detailed()
			.run()
			.unwrap_err();
		assert_eq!(e.attempts, three());
		assert_eq!(e.reason, StopReason::AttemptsExhausted);
		let e = policy()
			.budget(Duration::ZERO)
			.retry(|| Err::<(), _>(7))
			.when(|_| false)
			.detailed()
			.run()
			.unwrap_err();
		assert_eq!(e.reason, StopReason::NotRetryable);
		assert_eq!(e.attempts, NonZeroU32::MIN);
		let e = policy()
			.budget(Duration::ZERO)
			.retry(|| Err::<(), _>(7))
			.detailed()
			.run()
			.unwrap_err();
		assert_eq!(e.reason, StopReason::BudgetExhausted);
		let e = Retry::new()
			.with_backoff(Stop)
			.retry(|| Err::<(), _>(7))
			.detailed()
			.run()
			.unwrap_err();
		assert_eq!(e.reason, StopReason::BackoffStopped);
	}
	#[test]
	fn hooks_predicates_operations_and_sleeps_are_ordered() {
		let events = RefCell::new(Vec::new());
		let mut n = 0;
		let result = runtime().block_on(
			policy()
				.retry(|| {
					n += 1;
					events.borrow_mut().push("op");
					ready(Err::<(), _>(n))
				})
				.when(|_| {
					events.borrow_mut().push("predicate");
					true
				})
				.on_retry(|error, info| {
					assert_eq!(*error, info.attempt.number.get());
					assert_eq!(info.delay, Duration::ZERO);
					events.borrow_mut().push("hook");
				})
				.run_async(|delay| {
					assert_eq!(delay, Duration::ZERO);
					events.borrow_mut().push("sleep");
					ready(())
				}),
		);
		assert_eq!(result, Err(3));
		assert_eq!(
			*events.borrow(),
			[
				"op",
				"predicate",
				"hook",
				"sleep",
				"op",
				"predicate",
				"hook",
				"sleep",
				"op",
				"predicate"
			]
		);
	}
	#[test]
	fn budget_rejects_sleep_and_includes_runtime() {
		let mut sleeps = Vec::new();
		let e = runtime()
			.block_on(
				Retry::new()
					.with_backoff(Duration::from_secs(1))
					.budget(Duration::from_millis(1))
					.retry(|| ready(Err::<(), _>("last")))
					.detailed()
					.run_async(|d| {
						sleeps.push(d);
						ready(())
					}),
			)
			.unwrap_err();
		assert_eq!(e.reason, StopReason::BudgetExhausted);
		assert_eq!(sleeps, []);
		let e = policy()
			.budget(Duration::from_millis(1))
			.retry(|| {
				std::thread::sleep(Duration::from_millis(2));
				Err::<(), _>(())
			})
			.detailed()
			.run()
			.unwrap_err();
		assert_eq!(e.reason, StopReason::BudgetExhausted);
		assert!(e.elapsed >= Duration::from_millis(2));
		assert_eq!(
			policy()
				.budget(Duration::ZERO)
				.retry(|| Ok::<_, ()>(9))
				.run(),
			Ok(9)
		);
	}
	#[test]
	fn budget_rechecked_after_slow_hook_and_sleeper() {
		for hook_slow in [false, true] {
			let mut calls = 0;
			let mut sleeps = 0;
			let e = policy()
				.budget(Duration::from_millis(10))
				.retry(|| {
					calls += 1;
					if calls == 1 {
						Err("retained")
					} else {
						Ok(42)
					}
				})
				.on_retry(|_, _| {
					if hook_slow {
						std::thread::sleep(Duration::from_millis(30));
					}
				})
				.detailed()
				.run_with_sleep(|_| {
					sleeps += 1;
					std::thread::sleep(Duration::from_millis(30));
				})
				.unwrap_err();
			assert_eq!(calls, 1);
			assert_eq!(sleeps, usize::from(!hook_slow));
			assert_eq!(e.error, "retained");
			assert_eq!(e.attempts, NonZeroU32::MIN);
			assert_eq!(e.reason, StopReason::BudgetExhausted);
			// The independent reviewer's zero-delay future-factory reproducer.
			let mut calls = 0;
			let mut sleeps = 0;
			let e = runtime()
				.block_on(
					policy()
						.budget(Duration::from_millis(10))
						.retry(|| {
							calls += 1;
							ready(if calls == 1 { Err("retained") } else { Ok(42) })
						})
						.on_retry(|_, _| {
							if hook_slow {
								std::thread::sleep(Duration::from_millis(30));
							}
						})
						.detailed()
						.run_async(|_| {
							sleeps += 1;
							tokio::time::sleep(Duration::from_millis(30))
						}),
				)
				.unwrap_err();
			assert_eq!(calls, 1);
			assert_eq!(sleeps, usize::from(!hook_slow));
			assert_eq!(e.error, "retained");
			assert_eq!(e.attempts, NonZeroU32::MIN);
			assert_eq!(e.reason, StopReason::BudgetExhausted);
		}
	}

	#[test]
	fn lending_runner_rechecks_budget_after_hook_and_sleep() {
		for hook_slow in [false, true] {
			let mut calls = 0;
			let mut sleeps = 0;
			let e = runtime()
				.block_on(
					policy()
						.budget(Duration::from_millis(10))
						.retry_async_borrowing(async || {
							calls += 1;
							if calls == 1 {
								Err("retained")
							} else {
								Ok(42)
							}
						})
						.on_retry(|_, _| {
							if hook_slow {
								std::thread::sleep(Duration::from_millis(30));
							}
						})
						.detailed()
						.run_async_borrowing(|_| {
							sleeps += 1;
							tokio::time::sleep(Duration::from_millis(30))
						}),
				)
				.unwrap_err();
			assert_eq!(calls, 1);
			assert_eq!(sleeps, usize::from(!hook_slow));
			assert_eq!(e.attempts, NonZeroU32::MIN);
			assert_eq!(e.reason, StopReason::BudgetExhausted);
			assert_eq!(e.into_inner(), "retained");
		}
	}

	#[test]
	fn extension_blocking_async_and_lending() {
		let p = policy();
		let mut n = 0;
		assert_eq!(
			(|| {
				n += 1;
				if n < 3 {
					Err(())
				} else {
					Ok(42)
				}
			})
			.retry(&p)
			.run(),
			Ok(42)
		);
		let mut n = 0;
		assert_eq!(
			runtime().block_on(
				(|| {
					n += 1;
					ready(if n < 3 { Err(()) } else { Ok(42) })
				})
				.retry(&p)
				.run_async(|_| ready(()))
			),
			Ok(42)
		);
		let mut n = 0;
		assert_eq!(
			runtime().block_on(
				p.retry_async_borrowing(async || {
					n += 1;
					ready(()).await;
					if n < 3 {
						Err(())
					} else {
						Ok(42)
					}
				})
				.run_async_borrowing(|_| ready(()))
			),
			Ok(42)
		);
	}
	#[test]
	fn no_sleep_or_hook_after_success_or_rejected_error() {
		let mut hooks = 0;
		let mut sleeps = Vec::new();
		let value = runtime().block_on(
			policy()
				.retry(|| ready(Ok::<_, ()>(42)))
				.on_retry(|(), _| hooks += 1)
				.run_async(|d| {
					sleeps.push(d);
					ready(())
				}),
		);
		assert_eq!(value, Ok(42));
		let error = runtime().block_on(
			policy()
				.retry(|| ready(Err::<(), _>("fatal")))
				.when(|_| false)
				.on_retry(|_, _| hooks += 1)
				.run_async(|d| {
					sleeps.push(d);
					ready(())
				}),
		);
		assert_eq!(error, Err("fatal"));
		assert_eq!(hooks, 0);
		assert_eq!(sleeps, []);
	}
	#[test]
	fn unlimited_removes_attempt_limit_and_metadata_cannot_overflow() {
		let p = policy().unlimited();
		let mut n = 0;
		assert_eq!(
			p.retry(|| {
				n += 1;
				if n < 5 {
					Err(())
				} else {
					Ok(n)
				}
			})
			.run(),
			Ok(5)
		);
		let mut execution = p.retry(|| Err::<(), _>(())).detailed();
		let mut state = State {
			start: Instant::now(),
			number: NonZeroU32::new(u32::MAX).unwrap(),
		};
		let error = execution
			.failed(&mut state, Instant::now(), ())
			.unwrap_err();
		assert_eq!(error.reason, StopReason::AttemptsExhausted);
		assert_eq!(error.attempts.get(), u32::MAX);
	}

	#[test]
	fn default_async_future_is_send() {
		fn assert_send<T: Send>(_: &T) {}
		let future = policy()
			.retry(|| ready(Err::<(), _>(())))
			.run_async(|_| ready(()));
		assert_send(&future);
		assert_eq!(runtime().block_on(future), Err(()));
	}
	#[test]
	fn measured_state_and_injected_rng_are_per_execution() {
		let p = Retry::new()
			.with_backoff(BinaryExponential::new(Slot::measured()))
			.max_attempts(three());
		for _ in 0..2 {
			let mut delays = Vec::new();
			let e = runtime().block_on(
				p.retry(|| ready(Err::<(), _>(7)))
					.rng(SmallRng::seed_from_u64(42))
					.run_async(|d| {
						delays.push(d);
						ready(())
					}),
			);
			assert_eq!(e, Err(7));
			assert_eq!(delays.len(), 2);
		}
		// Configured policy is untouched even after both measured executions.
		let mut original = p.backoff.clone();
		let mut fresh = BinaryExponential::new(Slot::measured());
		let attempt = Attempt::new(three(), Duration::from_secs(2), Duration::ZERO);
		assert_eq!(
			original.next_delay(&attempt, &mut SmallRng::seed_from_u64(1)),
			fresh.next_delay(&attempt, &mut SmallRng::seed_from_u64(1))
		);
	}
	#[test]
	fn error_display_and_source() {
		use std::error::Error;
		let e = policy()
			.retry(|| Err::<(), _>(std::io::Error::other("failure")))
			.detailed()
			.run()
			.unwrap_err();
		assert!(e.to_string().contains("3 attempts"));
		assert_eq!(e.source().unwrap().to_string(), "failure");
	}
	#[cfg(feature = "tokio")]
	#[test]
	fn tokio_runner() {
		let mut calls = 0;
		let value = runtime().block_on(
			policy()
				.retry_async_borrowing(async || {
					calls += 1;
					tokio::time::sleep(Duration::ZERO).await;
					if calls < 3 {
						Err(())
					} else {
						Ok(calls)
					}
				})
				.run_tokio_borrowing(),
		);
		assert_eq!(value, Ok(3));
		assert_eq!(
			runtime().block_on(policy().retry(|| ready(Err::<(), _>(()))).run_tokio()),
			Err(())
		);
	}
}
