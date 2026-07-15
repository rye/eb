use std::ffi::OsString;
use std::process::Command;

pub(crate) fn app() -> clap::Command {
	use clap::{Arg, Command};

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
		.allow_external_subcommands(true)
}

pub(crate) fn command(arg_matches: &clap::ArgMatches) -> Option<Command> {
	match arg_matches.subcommand() {
		Some((name, matches)) => {
			let mut command = Command::new(name);

			if let Some(subcommand_args) = matches.get_many::<OsString>("") {
				for arg in subcommand_args {
					command.arg(arg);
				}
			}

			Some(command)
		}
		_ => None,
	}
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

		let mut args = command.get_args();
		assert_eq!(args.next(), None);
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

		let mut args = command.get_args();
		assert_eq!(args.next(), None);
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

		let mut args = command.get_args();
		assert_eq!(args.next(), Some("-x".as_ref()));
		assert_eq!(args.next(), Some("10".as_ref()));
		assert_eq!(args.next(), None);
	}
}
