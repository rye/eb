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
			// Once the program is seen, every remaining token belongs to it.
			Arg::new("command")
				.num_args(1..)
				.trailing_var_arg(true)
				.allow_hyphen_values(true)
				.value_parser(value_parser!(OsString)),
		)
}

pub(crate) fn command(arg_matches: &clap::ArgMatches) -> Option<Command> {
	let mut argv = arg_matches.get_many::<OsString>("command")?;
	let mut command = Command::new(argv.next()?);
	command.args(argv);
	Some(command)
}

pub(crate) fn max(arg_matches: &clap::ArgMatches) -> eb::Result<Option<u32>> {
	match arg_matches.get_one::<String>("max") {
		None => Ok(None),
		Some(arg) => match arg.parse() {
			Ok(max) => Ok(Some(max)),
			Err(e) => Err(eb::Error::InvalidMaxValue(e.to_string())),
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
			Err(eb::Error::InvalidMaxValue(
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
