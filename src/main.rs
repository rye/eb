#![deny(clippy::all)]
#![warn(clippy::cargo, clippy::pedantic, clippy::cognitive_complexity)]

mod cli;

use cli::command::{CommandRunner, Outcome};
use eb::{
	backoff::{BinaryExponential, Slot},
	Retry,
};
use log::info;
use std::{num::NonZeroU32, process::Command};

fn main() -> ExecutionResult {
	let code = execute()?;
	if code != 0 {
		std::process::exit(code);
	}
	Ok(())
}

fn execute() -> anyhow::Result<i32> {
	#[cfg(feature = "simple_logger")]
	simple_logger::SimpleLogger::new().init().unwrap();

	let matches = cli::argv::app().get_matches();
	let command: Command = cli::argv::command(&matches).ok_or(Error::NoCommandGiven)?;
	let max = cli::argv::max(&matches)?;
	// Preserve the existing zero-limit behavior: no child is started.
	if max == Some(0) {
		return Ok(0);
	}
	let mut runner = CommandRunner::new(command);
	// The slot time is the mean runtime of the failed attempts so far.
	let mut policy = Retry::new().with_backoff(BinaryExponential::new(Slot::measured()));
	if let Some(max) = max.and_then(NonZeroU32::new) {
		policy = policy.max_attempts(max);
	}
	let result = policy
		.retry(|| match runner.attempt() {
			Outcome::Complete(_) => Ok(()),
			outcome => Err(outcome),
		})
		// Signals are fatal; only ordinary non-zero exits are retried.
		.when(|outcome: &Outcome| matches!(outcome, Outcome::Retry(_)))
		.on_retry(|outcome, info| {
			if let Outcome::Retry(status) = outcome {
				info!(
					"Child exited with status {status}; retrying after {:?}",
					info.delay
				);
			}
		})
		.run();
	match result {
		Ok(()) => Ok(0),
		Err(Outcome::Retry(status)) => Ok(status.code().unwrap_or(1)),
		Err(_) => Err(Error::ChildProcessTerminatedWithSignal.into()),
	}
}

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub(crate) enum Error {
	#[error("no command given")]
	NoCommandGiven,
	#[error("child process terminated with signal")]
	ChildProcessTerminatedWithSignal,
	#[error("invalid max argument value: {0}")]
	InvalidMaxValue(String),
}
pub(crate) type Result<T> = std::result::Result<T, Error>;
type ExecutionResult = anyhow::Result<()>;
