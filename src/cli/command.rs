use std::process::{Command, ExitStatus};

/// The result of a single attempt to run a command.
pub(crate) enum Outcome {
	/// The command exited successfully.
	#[allow(
		dead_code,
		reason = "not used as of 2026-07-15 but might be used shortly"
	)]
	Complete(ExitStatus),
	/// The command exited unsuccessfully; retrying may help.
	Retry(ExitStatus),
	/// The command was terminated by a signal; retrying will not help.
	Fatal,
}

/// Classifies a completed command's exit status into an `Outcome`.
fn classify(status: ExitStatus) -> Outcome {
	match status.code() {
		Some(0) => Outcome::Complete(status),
		Some(_) => Outcome::Retry(status),
		None => Outcome::Fatal,
	}
}

/// Encapsulates running a single external command to completion.
pub(crate) struct CommandRunner {
	command: Command,
}

impl CommandRunner {
	pub(crate) fn new(command: Command) -> Self {
		Self { command }
	}

	pub(crate) fn attempt(&mut self) -> Outcome {
		let status: ExitStatus = self.command.status().expect("failed to execute process");

		classify(status)
	}
}

#[cfg(all(test, unix))]
mod tests {
	use super::{classify, Outcome};
	use std::os::unix::process::ExitStatusExt;
	use std::process::ExitStatus;

	#[test]
	fn zero_is_complete() {
		let status = ExitStatus::from_raw(0);

		assert!(matches!(classify(status), Outcome::Complete(_)));
	}

	#[test]
	fn nonzero_is_retry() {
		let status = ExitStatus::from_raw(1 << 8);

		assert!(matches!(classify(status), Outcome::Retry(_)));
	}

	#[test]
	fn signal_is_fatal() {
		let status = ExitStatus::from_raw(9);

		assert!(matches!(classify(status), Outcome::Fatal));
	}
}
