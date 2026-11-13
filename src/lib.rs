//! Reusable blocking and runtime-independent asynchronous retry policies.
//!
//! ```
//! use eb::{Retry, Retryable};
//! let policy = Retry::new();
//! assert_eq!((|| Ok::<_, ()>(42)).retry_blocking(&policy).run(), Ok(42));
//! ```
#![deny(clippy::all)]
#![warn(clippy::cargo, clippy::pedantic, clippy::cognitive_complexity)]
#![warn(missing_docs)]

/// Public RNG dependency (rand 0.10), shared with custom backoff implementations.
pub use rand;

pub mod backoff;
pub mod retry;
pub use retry::{Execution, Retry, RetryError, RetryInfo, Retryable, StopReason};
