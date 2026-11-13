# CLI integration tests

Run `cargo test --test cli` (also run automatically by the existing Cargo CI jobs).
The Unix execution tests use `/bin/sh` and a temporary directory per test. The
fixture uses shell builtins to count calls, record arguments, and succeed on a
specified call or terminate itself with SIGTERM. There are no randomized outcomes,
external fixture commands, sleeps, or timing assertions. A 10-second process timeout
bounds regressions that would otherwise retry forever. Linux and macOS support
these fixtures; portable CLI text tests also run elsewhere.

The tests cover no command, version text, immediate success, two failures before
success, retry exhaustion, invalid limits, fatal signals, and verbatim child
arguments. Invocation counts and captured child output establish execution behavior;
logger messages are deliberately not asserted. In particular, exhausting `-x 2`
currently exits **0** even though both child attempts fail. The test characterizes
that behavior; change its expectation when the CLI's exhaustion policy changes.

`snapshots/no-command.stderr` and `snapshots/version.stdout` are plain text
snapshots, compared with assert_cmd. To update one after an intentional wording
change, run `cargo test --test cli`, inspect the actual output in the assertion
failure, edit the corresponding file (preserving the final newline), and rerun.
The version snapshot substitutes `{{version}}` from Cargo package metadata so a
release version bump needs no snapshot change. Help formatting is not frozen.

These tests replace all three scripts formerly in `tests/smoke/`. No GitHub Actions
workflow invoked those shell scripts; Cargo discovers the Rust tests automatically.
