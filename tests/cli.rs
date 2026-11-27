#![warn(clippy::all, clippy::pedantic)]

use assert_cmd::Command;

fn eb() -> Command {
	let mut command = Command::new(env!("CARGO_BIN_EXE_eb"));
	command.env("NO_COLOR", "1").env("CLICOLOR", "0");
	command.timeout(std::time::Duration::from_secs(10));
	command
}

#[test]
fn no_command_is_an_error() {
	eb()
		.assert()
		.code(1)
		.stdout("")
		.stderr(include_str!("snapshots/no-command.stderr"));
}

#[test]
fn version() {
	// Keep the checked-in snapshot useful across package version bumps.
	let expected =
		include_str!("snapshots/version.stdout").replace("{{version}}", env!("CARGO_PKG_VERSION"));
	eb()
		.arg("--version")
		.assert()
		.success()
		.stdout(expected)
		.stderr("");
}

#[cfg(unix)]
#[path = "support/fixture.rs"]
mod fixture;

#[cfg(unix)]
mod execution {
	use super::{eb, fixture::Fixture};
	use predicates::str::contains;

	#[test]
	fn first_attempt_succeeds() {
		let fixture = Fixture::new("1");
		fixture
			.command(eb())
			.assert()
			.success()
			.stdout(contains("attempt 1\n"));
		fixture.assert_count(1);
	}

	#[test]
	fn retries_twice_then_succeeds() {
		let fixture = Fixture::new("3");
		fixture
			.command(eb())
			.assert()
			.success()
			.stdout(contains("attempt 3\n"));
		fixture.assert_count(3);
	}

	#[test]
	fn exhausted_limit_returns_last_child_code() {
		let fixture = Fixture::new("never");
		let mut command = eb();
		command.args(["-x", "2"]);
		fixture
			.command(command)
			.assert()
			.code(7)
			.stdout(contains("attempt 2\n"));
		fixture.assert_count(2);
	}

	#[test]
	fn invalid_limit_does_not_run_the_child() {
		let fixture = Fixture::new("1");
		let mut command = eb();
		command.args(["-x", "notanumber"]);
		fixture
			.command(command)
			.assert()
			.failure()
			.stdout("")
			.stderr(contains("invalid"));
		fixture.assert_count(0);
	}

	#[test]
	fn signal_termination_is_fatal_without_retry() {
		let fixture = Fixture::new("signal");
		fixture
			.command(eb())
			.assert()
			.code(1)
			.stdout(contains("attempt 1\n"))
			.stderr(contains("child process terminated with signal"));
		fixture.assert_count(1);
	}

	#[test]
	fn child_arguments_are_passed_verbatim() {
		let fixture = Fixture::new("1");
		fixture
			.command(eb())
			.args([
				"hello world",
				"",
				"-x",
				"5",
				"--help",
				"--",
				"'quoted'",
				"λ",
			])
			.assert()
			.success()
			.stdout(contains("attempt 1\n"));
		fixture.assert_count(1);
		fixture.assert_arguments("hello world\n\n-x\n5\n--help\n--\n'quoted'\nλ\n");
	}
}
