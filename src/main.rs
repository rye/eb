use eb::clamp;
use eb::SlotTime;

use clap::{App, AppSettings, Arg};
use rand::distributions::{Distribution, Uniform};

use log::{debug, error, info, trace};

use core::time::Duration;
use std::{
	process::{Command, ExitStatus},
	thread::sleep,
	time::Instant,
};

fn app<'a, 'b>() -> clap::App<'a, 'b> {
	App::new(env!("CARGO_PKG_NAME"))
		.about(env!("CARGO_PKG_DESCRIPTION"))
		.version(env!("CARGO_PKG_VERSION"))
		.author(env!("CARGO_PKG_AUTHORS"))
		.arg(
			Arg::with_name("max")
				.short("x")
				.takes_value(true)
				.help("limits the number of times command is executed"),
		)
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

fn main() -> eb::ExecutionResult {
	#[cfg(feature = "simple_logger")]
	simple_logger::SimpleLogger::new().init().unwrap();

	let max_n: u32 = 10;

	let matches = app().get_matches();

	let mut command: Command = command(&matches).ok_or(eb::Error::NoCommandGiven)?;

	let mut iterations: u32 = 0;
	let mut slot_time: Option<SlotTime> = None;
	let mut rng = rand::thread_rng();

	let max = match matches.value_of("max").map(|s| s.parse::<u32>()) {
		Some(Ok(v)) => Some(v),
		Some(Err(e)) => return Err(eb::Error::InvalidMaxValue(e.to_string())),
		None => None,
	};
	trace!("Beginning iteration...");

	loop {
		if let Some(true) = max.map(|v| iterations >= v) {
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
			let dur: Duration = (*dur * (iterations - 1) + elapsed) / iterations;
			slot_time = Some(SlotTime::AutoGenerated(dur));
		} else if slot_time.is_none() {
			slot_time = Some(SlotTime::AutoGenerated(elapsed));
		}

		let delay: Duration = match &slot_time {
			Some(SlotTime::UserSpecified(dur)) | Some(SlotTime::AutoGenerated(dur)) => {
				let distribution = Uniform::new(0, 2_u32.pow(clamp(iterations + 1, 0, max_n)) - 1);
				let multiplicand: u32 = distribution.sample(&mut rng);

				*dur * multiplicand
			}
			None => Duration::new(0, 0),
		};

		debug!("Sleeping for {}s", delay.as_secs_f64());

		sleep(delay);
	}
}
