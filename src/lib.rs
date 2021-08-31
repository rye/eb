use core::{fmt::Debug, result, time::Duration};

pub enum SlotTime {
	UserSpecified(Duration),
	AutoGenerated(Duration),
}

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
	#[error("no command given")]
	NoCommandGiven,
	#[error("child process terminated with signal")]
	ChildProcessTerminatedWithSignal,
	#[error("invalid max argument value: {0}")]
	InvalidMaxValue(String),
}

pub type Result<T> = result::Result<T, Error>;

pub type ExecutionResult = anyhow::Result<()>;

#[cfg(test)]
mod tests {}

pub mod backoff;
