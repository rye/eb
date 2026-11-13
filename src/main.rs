#![deny(clippy::all)]
#![warn(clippy::cargo, clippy::pedantic, clippy::cognitive_complexity)]

mod cli;

use cli::command::{CommandRunner, Outcome};
use eb::{
	backoff::{BackoffExt, BinaryExponential, Jitter, Slot},
	Retry,
};
use log::info;
use std::{num::NonZeroU32, process::Command, time::Duration};

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
	let stop_on: Vec<i32> = matches
		.get_many::<i32>("stop-on")
		.into_iter()
		.flatten()
		.copied()
		.collect();
	let mut runner = CommandRunner::new(command);
	let backoff = BinaryExponential::new(
		matches
			.get_one::<Duration>("slot-time")
			.copied()
			.map_or(Slot::measured(), Slot::Fixed),
	)
	.jitter(if matches.get_flag("discrete") {
		Jitter::Discrete
	} else {
		Jitter::Continuous
	});
	let mut policy =
		Retry::new().with_backoff(backoff.between_starts_if(matches.get_flag("sync-slots")));
	if let Some(max) = max.and_then(NonZeroU32::new) {
		policy = policy.max_attempts(max);
	}
	let result = policy
		.retry(|| match runner.attempt() {
			Outcome::Complete(_) => Ok(()),
			outcome => Err(outcome),
		})
		.when(|outcome: &Outcome| match outcome {
			Outcome::Retry(status) => !status.code().is_some_and(|code| stop_on.contains(&code)),
			_ => false,
		})
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
