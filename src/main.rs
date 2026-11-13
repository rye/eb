#![deny(clippy::all)]
#![warn(clippy::cargo, clippy::pedantic, clippy::cognitive_complexity)]

mod cli;

use cli::command::{CommandRunner, Outcome};
use eb::{
	backoff::{BinaryExponential, Slot},
	Retry,
};
use log::info;
use std::{
	num::NonZeroU32,
	process::{Command, ExitCode},
};

fn main() -> anyhow::Result<ExitCode> {
	#[cfg(feature = "simple_logger")]
	simple_logger::SimpleLogger::new().init().unwrap();

	let matches = cli::argv::app().get_matches();
	let command: Command = cli::argv::command(&matches).ok_or(Error::NoCommandGiven)?;
	let max = cli::argv::max(&matches)?;
	// Preserve the existing zero-limit behavior: no child is started.
	if max == Some(0) {
		return Ok(ExitCode::SUCCESS);
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
		Ok(()) => Ok(ExitCode::SUCCESS),
		Err(Outcome::Retry(status)) => Ok(exit_code(
			status.code().expect("Outcome::Retry implies an exit code"),
		)),
		Err(_) => Err(Error::ChildProcessTerminatedWithSignal.into()),
	}
}

/// Converts a child's exit code into this process's exit code. Codes that do
/// not fit in a `u8` (possible only on Windows) become a generic failure.
fn exit_code(code: i32) -> ExitCode {
	u8::try_from(code).map_or(ExitCode::FAILURE, ExitCode::from)
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
