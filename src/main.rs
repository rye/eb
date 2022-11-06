use eb::SlotTime;

use rand::distributions::Uniform;

use log::{debug, error, info, trace};

use core::time::Duration;
use std::{
	process::{Command, ExitStatus},
	thread::sleep,
	time::Instant,
};

fn app() -> clap::Command {
	use clap::{Arg, Command};

	Command::new(env!("CARGO_PKG_NAME"))
		.about(env!("CARGO_PKG_DESCRIPTION"))
		.version(env!("CARGO_PKG_VERSION"))
		.author(env!("CARGO_PKG_AUTHORS"))
		.arg(
			Arg::new("max")
				.short('x')
				.takes_value(true)
				.allow_hyphen_values(true)
				.number_of_values(1)
				.help("limits the number of times command is executed"),
		)
		.allow_external_subcommands(true)
}

fn command(arg_matches: &clap::ArgMatches) -> Option<Command> {
	match arg_matches.subcommand() {
		Some((name, matches)) => {
			let mut command = Command::new(name);
			let args: Vec<&str> = matches.values_of("").map_or(Vec::new(), Iterator::collect);

			for arg in args {
				command.arg(arg);
			}

			Some(command)
		}
		_ => None,
	}
}

fn max(arg_matches: &clap::ArgMatches) -> eb::Result<Option<u32>> {
	match arg_matches.value_of("max") {
		None => Ok(None),
		Some(arg) => match arg.parse() {
			Ok(max) => Ok(Some(max)),
			Err(e) => Err(eb::Error::InvalidMaxValue(e.to_string())),
		},
	}
}

fn main() -> eb::ExecutionResult {
	#[cfg(feature = "simple_logger")]
	simple_logger::SimpleLogger::new().init().unwrap();

	let matches = app().get_matches();

	let mut command: Command = command(&matches).ok_or(eb::Error::NoCommandGiven)?;

	let mut iterations: u32 = 0;
	let mut slot_time: Option<SlotTime> = None;
	let mut rng = rand::thread_rng();

	let max: Option<u32> = max(&matches)?;

	let distribution = Uniform::new(0.0_f32, 1.0_f32);

	trace!("Beginning iteration...");

	loop {
		if let Some(true) = max.map(|max| iterations >= max) {
			break Ok(());
		}
		trace!("Starting iteration {}", iterations);

		let start: Instant = Instant::now();
		let status: ExitStatus = command.status().expect("failed to execute process");
		let elapsed: Duration = start.elapsed();

		iterations += 1;

		match status.code() {
			Some(0) => {
				info!("Child exited with status 0; finished.");
				break Ok(());
			}
			Some(code) => {
				info!("Child exited with status {}", code);
			}
			None => {
				error!("Child terminated by signal");
				break Err(eb::Error::ChildProcessTerminatedWithSignal.into());
			}
		}

		if let Some(SlotTime::AutoGenerated(dur)) = &slot_time {
			let dur: Duration = (*dur * (iterations - 1) + elapsed) / iterations;
			slot_time = Some(SlotTime::AutoGenerated(dur));
		} else if slot_time.is_none() {
			slot_time = Some(SlotTime::AutoGenerated(elapsed));
		}

		let delay: Duration = match &slot_time {
			Some(SlotTime::UserSpecified(dur) | SlotTime::AutoGenerated(dur)) => {
				eb::backoff::wait_size_truncated(dur, iterations, 10_u32, &mut rng, &distribution)
			}
			None => Duration::new(0, 0),
		};

		debug!("Sleeping for {}s", delay.as_secs_f64());

		sleep(delay);
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
		// TODO Test the actual population of `command` here.
		// Gated behind https://github.com/rust-lang/rust/issues/44434.
	}

	#[test]
	fn no_other_args_some() {
		let argv = ["eb", "cmd"];
		let matches = t_matches_from(&argv);
		let command = command(&matches);

		assert!(command.is_some());
		// TODO Test the actual population of `command` here.
		// Gated behind https://github.com/rust-lang/rust/issues/44434.
	}

	#[test]
	fn some_max_pre() {
		let argv = ["eb", "-x", "10", "cmd"];
		let matches = t_matches_from(&argv);
		let command = command(&matches);
		let max = super::max(&matches);

		assert!(command.is_some());
		// TODO Test the actual population of `command` here.
		// Gated behind https://github.com/rust-lang/rust/issues/44434.
		assert_eq!(max, Ok(Some(10_u32)));
	}

	#[test]
	fn some_max_post_none() {
		let argv = ["eb", "cmd", "-x", "10"];
		let matches = t_matches_from(&argv);
		let command = command(&matches);
		let max = super::max(&matches);

		assert!(command.is_some());
		// TODO Test the actual population of `command` here.
		// Gated behind https://github.com/rust-lang/rust/issues/44434.
		assert_eq!(max, Ok(None));
	}
}
