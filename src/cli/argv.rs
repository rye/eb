use std::ffi::OsString;
use std::process::Command;

pub(crate) fn app() -> clap::Command {
	use clap::{value_parser, Arg, Command};

	Command::new(env!("CARGO_PKG_NAME"))
		.about(env!("CARGO_PKG_DESCRIPTION"))
		.version(env!("CARGO_PKG_VERSION"))
		.author(env!("CARGO_PKG_AUTHORS"))
		.arg(
			Arg::new("max")
				.short('x')
				.num_args(1)
				.allow_negative_numbers(true)
				.help("limits the number of times command is executed"),
		)
		.arg(
			Arg::new("slot-time")
				.short('i')
				.long("slot-time")
				.value_name("SLOT_TIME")
				.value_parser(parse_duration)
				.help("Fixed slot duration (e.g. 500ms, 1.5s, 1m; bare numbers are seconds)"),
		)
		.arg(
			Arg::new("discrete")
				.long("discrete")
				.action(clap::ArgAction::SetTrue)
				.help("Use integer-slot IEEE 802.3 style jitter"),
		)
		.arg(
			Arg::new("sync-slots")
				.long("sync-slots")
				.action(clap::ArgAction::SetTrue)
				.help("Subtract attempt runtime from delays, clamping at zero"),
		)
		.arg(
			Arg::new("stop-on")
				.long("stop-on")
				.value_name("CODE")
				.value_delimiter(',')
				.action(clap::ArgAction::Append)
				.value_parser(value_parser!(i32).range(0..))
				.help("Stop on these exit codes (repeatable or comma-separated)"),
		)
		.arg(
			// Once the program is seen, every remaining token belongs to it.
			Arg::new("command")
				.num_args(1..)
				.trailing_var_arg(true)
				.allow_hyphen_values(true)
				.value_parser(value_parser!(OsString)),
		)
}

// Decimal arithmetic avoids float rounding and rejects unrepresentable durations.
fn parse_duration(input: &str) -> Result<std::time::Duration, String> {
	let invalid =
		|| "expected a nonnegative duration (e.g. 500ms, 1.5s, 1m; bare seconds)".to_owned();
	let split = input
		.find(|c: char| !c.is_ascii_digit() && c != '.')
		.unwrap_or(input.len());
	let (number, unit) = input.split_at(split);
	let nanos: u128 = match unit {
		"ns" => 1,
		"us" => 1_000,
		"ms" => 1_000_000,
		"" | "s" => 1_000_000_000,
		"m" => 60_000_000_000,
		"h" => 3_600_000_000_000,
		_ => return Err(invalid()),
	};
	let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
	if whole.is_empty() || fraction.len() > 9 || (number.contains('.') && fraction.is_empty()) {
		return Err(invalid());
	}
	let scale = 10_u128.pow(u32::try_from(fraction.len()).map_err(|_| invalid())?);
	let whole = whole.parse::<u128>().map_err(|_| invalid())?;
	let fraction = if fraction.is_empty() {
		0
	} else {
		fraction.parse::<u128>().map_err(|_| invalid())?
	};
	let value = whole
		.checked_mul(scale)
		.and_then(|v| v.checked_add(fraction))
		.and_then(|v| v.checked_mul(nanos))
		.ok_or_else(invalid)?;
	if value % scale != 0 {
		return Err(invalid());
	}
	let value = value / scale;
	Ok(std::time::Duration::new(
		u64::try_from(value / 1_000_000_000).map_err(|_| invalid())?,
		u32::try_from(value % 1_000_000_000).map_err(|_| invalid())?,
	))
}

#[cfg(test)]
mod durations {
	use super::parse_duration;
	use std::time::Duration;
	#[test]
	fn valid() {
		for (input, expected) in [
			("500ms", 500),
			("2s", 2000),
			("1.5s", 1500),
			("1m", 60000),
			("2", 2000),
			("0s", 0),
		] {
			assert_eq!(parse_duration(input), Ok(Duration::from_millis(expected)));
		}
	}
	#[test]
	fn invalid() {
		for input in [
			"",
			"-1s",
			"NaN",
			"inf",
			"1.2.3s",
			"1fortnight",
			"1.s",
			".5s",
			"0.1ns",
			"18446744073709551616s",
		] {
			assert!(parse_duration(input).is_err(), "{input}");
		}
	}
}

pub(crate) fn command(arg_matches: &clap::ArgMatches) -> Option<Command> {
	let mut argv = arg_matches.get_many::<OsString>("command")?;
	let mut command = Command::new(argv.next()?);
	command.args(argv);
	Some(command)
}

pub(crate) fn max(arg_matches: &clap::ArgMatches) -> crate::Result<Option<u32>> {
	match arg_matches.get_one::<String>("max") {
		None => Ok(None),
		Some(arg) => match arg.parse() {
			Ok(max) => Ok(Some(max)),
			Err(e) => Err(crate::Error::InvalidMaxValue(e.to_string())),
		},
	}
}

#[cfg(test)]
fn t_matches_from(argv: &[&str]) -> clap::ArgMatches {
	let app = app();
	app.get_matches_from(argv)
}

#[cfg(test)]
mod max {
	use super::{max, t_matches_from};

	#[test]
	fn not_specified() {
		let argv = ["eb", "cmd"];
		let matches = t_matches_from(&argv);
		let max = max(&matches);

		assert_eq!(max, Ok(None));
	}

	#[test]
	fn short_valid() {
		let argv = ["eb", "-x", "10", "cmd"];
		let matches = t_matches_from(&argv);
		let max = max(&matches);

		assert_eq!(max, Ok(Some(10_u32)));
	}

	#[test]
	fn short_invalid() {
		let argv = ["eb", "-x", "notanumber", "cmd"];
		let matches = t_matches_from(&argv);
		let max = max(&matches);

		assert_eq!(
			max,
			Err(crate::Error::InvalidMaxValue(
				"invalid digit found in string".to_string()
			))
		);
	}
}

#[cfg(test)]
mod command {
	use super::{command, t_matches_from};

	#[test]
	fn not_specified() {
		let argv = ["eb"];
		let matches = t_matches_from(&argv);
		let command = command(&matches);

		assert!(command.is_none());
	}

	#[test]
	fn no_other_args_some() {
		let argv = ["eb", "cmd"];
		let matches = t_matches_from(&argv);
		let command = command(&matches);

		assert!(command.is_some());

		let command = command.unwrap();
		assert_eq!(command.get_program(), "cmd");

		let mut child_args = command.get_args();
		assert_eq!(child_args.next(), None);
	}

	#[test]
	fn some_max_pre() {
		let argv = ["eb", "-x", "10", "cmd"];
		let matches = t_matches_from(&argv);
		let command = command(&matches);
		let max = super::max(&matches);

		assert_eq!(max, Ok(Some(10_u32)));
		assert!(command.is_some());

		let command = command.unwrap();
		assert_eq!(command.get_program(), "cmd");

		let mut child_args = command.get_args();
		assert_eq!(child_args.next(), None);
	}

	#[test]
	fn some_max_post_none() {
		let argv = ["eb", "cmd", "-x", "10"];
		let matches = t_matches_from(&argv);
		let command = command(&matches);
		let max = super::max(&matches);

		assert_eq!(max, Ok(None));
		assert!(command.is_some());

		let command = command.unwrap();
		assert_eq!(command.get_program(), "cmd");

		let mut child_args = command.get_args();
		assert_eq!(child_args.next(), Some("-x".as_ref()));
		assert_eq!(child_args.next(), Some("10".as_ref()));
		assert_eq!(child_args.next(), None);
	}
}

#[cfg(test)]
mod trailing_args {
	use super::{app, command, max, t_matches_from};

	#[test]
	fn child_help_and_version_are_passed_through() {
		for flag in ["--help", "-h", "--version"] {
			let matches = t_matches_from(&["eb", "cmd", flag]);
			let command = command(&matches).unwrap();
			assert_eq!(command.get_program(), "cmd");
			assert_eq!(command.get_args().collect::<Vec<_>>(), [flag]);
		}
	}

	#[test]
	fn separator_before_command_is_consumed() {
		let matches = t_matches_from(&["eb", "--", "cmd", "--flag"]);
		let command = command(&matches).unwrap();
		assert_eq!(command.get_program(), "cmd");
		assert_eq!(command.get_args().collect::<Vec<_>>(), ["--flag"]);
	}

	#[test]
	fn hyphenated_program_after_separator() {
		let matches = t_matches_from(&["eb", "-x", "3", "--", "-weird-program-name"]);
		assert_eq!(max(&matches), Ok(Some(3)));
		let command = command(&matches).unwrap();
		assert_eq!(command.get_program(), "-weird-program-name");
		assert_eq!(command.get_args().count(), 0);
	}

	#[test]
	fn separator_after_command_is_preserved() {
		let matches = t_matches_from(&["eb", "cmd", "--", "--flag"]);
		let command = command(&matches).unwrap();
		assert_eq!(command.get_args().collect::<Vec<_>>(), ["--", "--flag"]);
	}

	#[cfg(unix)]
	#[test]
	fn non_utf8_program_and_arguments_are_preserved() {
		use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

		let program = OsStr::from_bytes(b"cmd\xff");
		let argument = OsStr::from_bytes(b"arg\xfe");
		let matches = app()
			.try_get_matches_from([OsStr::new("eb"), program, argument])
			.unwrap();
		let command = command(&matches).unwrap();
		assert_eq!(command.get_program(), program);
		assert_eq!(command.get_args().collect::<Vec<_>>(), [argument]);
	}

	#[test]
	fn wrapper_help_is_available() {
		let error = app().try_get_matches_from(["eb", "--help"]).unwrap_err();
		assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
		let help = error.to_string();
		assert!(help.contains("Usage:"));
		assert!(help.contains("-x <max>"));
		assert!(help.contains("[command]..."));
	}

	#[test]
	fn missing_max_value_has_context() {
		let error = app().try_get_matches_from(["eb", "-x"]).unwrap_err();
		let message = error.to_string();
		assert!(message.contains("-x <max>"));
		assert!(message.contains("none was supplied"));
		assert!(message.contains("--help"));
	}
}
