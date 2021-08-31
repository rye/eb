use eb::SlotTime;

use rand::distributions::Uniform;

use log::{debug, error, info, trace};

use core::time::Duration;
use std::{
	process::{Command, ExitStatus},
	thread::sleep,
	time::Instant,
};

fn app<'a, 'b>() -> clap::App<'a, 'b> {
	use clap::{App, AppSettings, Arg};

	App::new(env!("CARGO_PKG_NAME"))
		.about(env!("CARGO_PKG_DESCRIPTION"))
		.version(env!("CARGO_PKG_VERSION"))
		.author(env!("CARGO_PKG_AUTHORS"))
		.arg(
			Arg::with_name("max")
				.short("x")
				.takes_value(true)
				.allow_hyphen_values(true)
				.number_of_values(1)
				.help("limits the number of times command is executed"),
		)
		.setting(AppSettings::AllowExternalSubcommands)
}

fn command(arg_matches: &clap::ArgMatches) -> Option<Command> {
	match arg_matches.subcommand() {
		(name, Some(matches)) => {
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
				break Err(eb::Error::ChildProcessTerminatedWithSignal);
			}
		}

		if let Some(SlotTime::AutoGenerated(dur)) = &slot_time {
			let dur: Duration = (*dur * (iterations - 1) as u32 + elapsed) / iterations as u32;
			slot_time = Some(SlotTime::AutoGenerated(dur));
		} else if slot_time.is_none() {
			slot_time = Some(SlotTime::AutoGenerated(elapsed));
		}

		let delay: Duration = match &slot_time {
			Some(SlotTime::UserSpecified(dur)) | Some(SlotTime::AutoGenerated(dur)) => {
				eb::backoff::wait_size_truncated(dur, iterations, 10_u32, &mut rng, &distribution)
			}
			None => Duration::new(0, 0),
		};

		debug!("Sleeping for {}s", delay.as_secs_f64());

		sleep(delay);
	}
}
