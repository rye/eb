#![warn(clippy::all, clippy::pedantic)]

use assert_cmd::Command;

fn eb() -> Command {
	let mut command = Command::new(env!("CARGO_BIN_EXE_eb"));
	command.env("NO_COLOR", "1").env("CLICOLOR", "0");
	command.args(["-i", "0s"]);
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
	fn default_adaptive_timing_retries() {
		let fixture = Fixture::new("3");
		let mut command = super::Command::new(env!("CARGO_BIN_EXE_eb"));
		command.timeout(std::time::Duration::from_secs(10));
		fixture.command(command).assert().success();
		fixture.assert_count(3);
	}

	#[test]
	fn zero_limit_does_not_start_child() {
		let fixture = Fixture::new("never");
		let mut command = eb();
		command.args(["-x", "0"]);
		fixture.command(command).assert().success();
		fixture.assert_count(0);
	}

	#[test]
	fn documented_durations_are_accepted() {
		for value in ["500ms", "2s", "1.5s", "1m", "2"] {
			let fixture = Fixture::new("1");
			let mut command = super::Command::new(env!("CARGO_BIN_EXE_eb"));
			command.args(["-i", value]);
			fixture.command(command).assert().success();
			fixture.assert_count(1);
		}
	}

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
			.stderr("")
			.stdout(contains("attempt 2\n"));
		fixture.assert_count(2);
	}

	#[test]
	fn fixed_slots_and_invalid_slots() {
		for flag in ["-i", "--slot-time"] {
			let fixture = Fixture::new("3");
			let mut command = super::Command::new(env!("CARGO_BIN_EXE_eb"));
			command.args([flag, "1ms"]);
			fixture.command(command).assert().success();
			fixture.assert_count(3);
		}
		for duration in [
			"nonsense",
			"-1s",
			"999999999999999999999999999999999999999s",
		] {
			let fixture = Fixture::new("1");
			let mut command = super::Command::new(env!("CARGO_BIN_EXE_eb"));
			command.args(["-i", duration]);
			fixture.command(command).assert().failure();
			fixture.assert_count(0);
		}
	}

	#[test]
	fn synchronized_slots_retry_and_exhaust() {
		for (mode, code) in [("3", 0), ("never", 7)] {
			let fixture = Fixture::new(mode);
			let mut command = eb();
			command.args(["--sync-slots", "-x", "3"]);
			fixture.command(command).assert().code(code);
			fixture.assert_count(3);
		}
	}

	#[test]
	fn discrete_jitter_retries_and_exhausts() {
		for sync in [false, true] {
			for (mode, code) in [("3", 0), ("never", 7)] {
				let fixture = Fixture::new(mode);
				let mut command = eb();
				command.args(["--discrete", "-x", "3"]);
				if sync {
					command.arg("--sync-slots");
				}
				fixture.command(command).assert().code(code);
				fixture.assert_count(3);
			}
		}
	}

	#[test]
	fn stop_codes_are_repeatable_and_comma_separated() {
		for flags in [
			vec!["--stop-on", "7"],
			vec!["--stop-on", "1,7"],
			vec!["--stop-on", "1", "--stop-on", "7"],
		] {
			let fixture = Fixture::new("never");
			let mut command = eb();
			command.args(flags);
			fixture.command(command).assert().code(7);
			fixture.assert_count(1);
		}
	}

	#[test]
	fn unlisted_codes_retry_and_success_still_stops() {
		let fixture = Fixture::new("3");
		let mut command = eb();
		command.args(["--stop-on", "0,7"]);
		fixture.command(command).assert().success();
		fixture.assert_count(3);
	}

	#[test]
	fn invalid_stop_codes_do_not_start_child() {
		for code in ["-1", "text", "2147483648", "1,,7"] {
			let fixture = Fixture::new("1");
			let mut command = eb();
			command.args(["--stop-on", code]);
			fixture.command(command).assert().failure();
			fixture.assert_count(0);
		}
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
		let mut command = eb();
		command.args(["--stop-on", "7"]);
		fixture
			.command(command)
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
				"-i",
				"5",
				"--help",
				"--slot-time",
				"2s",
				"--sync-slots",
				"--stop-on",
				"7",
				"--",
				"'quoted'",
				"λ",
			])
			.assert()
			.success()
			.stdout(contains("attempt 1\n"));
		fixture.assert_count(1);
		fixture.assert_arguments("hello world\n\n-x\n5\n-i\n5\n--help\n--slot-time\n2s\n--sync-slots\n--stop-on\n7\n--\n'quoted'\nλ\n");
	}
}
