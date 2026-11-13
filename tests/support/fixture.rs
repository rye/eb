use assert_cmd::Command;
use std::{fs, path::PathBuf};
use tempfile::TempDir;

pub struct Fixture {
	directory: TempDir,
	mode: String,
}

impl Fixture {
	pub fn new(mode: &str) -> Self {
		let directory = tempfile::tempdir().unwrap();
		fs::write(directory.path().join("child.sh"), include_str!("child.sh")).unwrap();
		fs::write(directory.path().join("count"), "0\n").unwrap();
		Self {
			directory,
			mode: mode.to_owned(),
		}
	}

	fn path(&self, name: &str) -> PathBuf {
		self.directory.path().join(name)
	}

	pub fn command(&self, mut command: Command) -> Command {
		command
			.arg("/bin/sh")
			.arg(self.path("child.sh"))
			.arg(self.path("count"))
			.arg(self.path("arguments"))
			.arg(&self.mode);
		command
	}

	pub fn assert_count(&self, expected: u32) {
		let count = fs::read_to_string(self.path("count")).unwrap();
		assert_eq!(count.trim().parse::<u32>().unwrap(), expected);
	}

	pub fn assert_arguments(&self, expected: &str) {
		assert_eq!(
			fs::read_to_string(self.path("arguments")).unwrap(),
			expected
		);
	}
}
