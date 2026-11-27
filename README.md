# eb

`eb` is a small utility that runs a command with exponential backoff.

`eb` retries until the command succeeds, an attempt limit is exhausted, a configured stop code is returned, or the child is terminated by a signal. Exhaustion and stop codes return the last child exit code without an extra error diagnostic. Signal termination is fatal and returns status 1 with a diagnostic.

You can use `eb` much like `watch`:

```console
$ eb -- ssh -oConnectTimeout=0 host
```

## CLI usage

The CLI is provided by the `cli` Cargo feature, enabled by default.

```text
eb [OPTIONS] [--] <command> [args...]
```

- `-x MAX` limits total attempts, including the first. Omit it for unlimited
  attempts. `-x 0` runs nothing and returns success.
- `-i SLOT_TIME`, `--slot-time SLOT_TIME` sets a fixed slot instead of the CLI's
  adaptive average. Examples: `500ms`, `2s`, `1.5s`, `1m`; bare numbers mean
  seconds. Units are `ns`, `us`, `ms`, `s`, `m`, `h`. Nonnegative decimal values
  with up to nine fractional digits must resolve to whole nanoseconds and fit
  a Rust `Duration`. Zero disables delays.
- `--sync-slots` subtracts the last attempt's runtime from the randomized delay,
  clamping at zero. Delays are measured from the failed attempt's start; by
  default the entire delay follows the attempt. With the default measured slot,
  the first retry has no sleep when this flag is enabled.
- `--discrete` selects uniformly random integer slot counts, as in IEEE 802.3.
  The default selects a continuous fraction of the backoff window.
- `--stop-on CODE` stops immediately on a listed exit code, returning that code.
  Repeat the option or use commas, e.g. `--stop-on 2,7 --stop-on 126`.
  Codes are nonnegative signed 32-bit integers; success (0) always stops.
- `-h`, `--help` shows help; `-V`, `--version` shows the version.

All wrapper options must precede the command. After the command, every argument
is passed through unchanged: `eb cmd -i 5` gives `-i 5` to `cmd`. An optional
`--` before the command allows a program name beginning with a hyphen.

```console
$ eb -x 5 -i 500ms --sync-slots --stop-on 2,126 -- my-command --verbose
```

Exit status is 0 on success (also for `-x 0`), the last child's exit code on
attempt exhaustion, or the matching `--stop-on` code. Exhaustion and stop codes
do not add an error diagnostic. A child killed by a signal stops retries and
returns status 1 with an error diagnostic. There is no sleep after success,
a stop code, a fatal signal, or the final permitted attempt.

## Installing

Install the CLI with:

```console
$ cargo install eb
```

For library-only use, disable the default CLI feature:

```toml
[dependencies]
eb = { version = "0.6", default-features = false }
```

## The algorithm

`eb` uses **exponent-truncated binary exponential backoff with continuous full
jitter, inspired by IEEE 802.3**. After failure `n` (the initial attempt is
attempt 1), it sets `k = min(n, 10)` and draws a uniform delay from
`[0, slot * (2^k - 1))`. Thus the first failure gives any fraction of one slot,
and the second any fraction of three slots. `--discrete` instead draws an integer
from `0` through `2^k - 1` and waits that many slots.

By default the CLI uses the arithmetic mean runtime of failed command attempts
so far as its slot, including the failure just observed and excluding sleeps.
`-i` fixes the slot instead. The exponent stops growing after ten failures, but
an adaptive slot can still grow; exponent truncation alone is not a duration
bound. There is no default attempt limit. `--sync-slots` subtracts the failed
attempt's runtime from the chosen delay, saturating at zero.

See the [backoff module documentation](https://docs.rs/eb/latest/eb/backoff/)
for exact formulas, scheduling and numerical behavior, slot selection, and a
comparison with Ethernet, AWS, and gRPC policies.

## Library usage

The library retries any blocking or async operation. A `Retry` policy holds
the backoff strategy and limits, and can be reused across operations with
different error types. Each call to `.retry_blocking(...)` or `.retry_async(...)` starts a fresh execution, so
measured slot times never leak between runs.

```toml
[dependencies]
eb = { version = "0.6", default-features = false }           # library only
# eb = { version = "0.6", default-features = false, features = ["tokio"] }
```

```rust
use eb::{backoff::{BinaryExponential, Slot}, Retry, Retryable};
use std::{num::NonZeroU32, time::Duration};

let policy = Retry::new()
	.with_backoff(BinaryExponential::new(Slot::Fixed(Duration::from_millis(100))))
	.max_attempts(NonZeroU32::new(5).unwrap());

let mut calls = 0;
let result = (|| {
	calls += 1;
	if calls < 3 { Err("transient") } else { Ok(calls) }
})
.retry_blocking(&policy)
.when(|error| *error == "transient")
.run();
assert_eq!(result, Ok(3));
```

Async operations are retried by building a fresh future for each attempt (a
completed `Future` cannot be polled again), with the sleep function of your
choice, or `.run_tokio()` with the `tokio` feature:

```rust
use eb::{Retry, Retryable};
use std::{future::ready, io, time::Duration};
# let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
# rt.block_on(async {
let policy = Retry::new().with_backoff(Duration::ZERO);
let body = (|| ready(Ok::<_, io::Error>("response")))
	.retry_async(&policy)
	.when(|error| error.kind() == io::ErrorKind::Interrupted)
	.on_retry(|error, info| eprintln!("{error}; retrying in {:?}", info.delay))
	.run_async(|_| ready(()))
	.await;
assert_eq!(body.unwrap(), "response");
# });
```

Async closures that borrow and mutate captured state use
`policy.retry_async_borrowing(async || { ... }).run_async_borrowing(sleep)`. Call `.detailed()`
before running to get a `RetryError` with the attempt count, elapsed time, and
stop reason. See the [API documentation](https://docs.rs/eb) for details.

## License

```text
eb: A command executor exercising exponential backoff
Copyright (C) 2019-2026 Kristofer J. Rye

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, version 3.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program.  If not, see <https://www.gnu.org/licenses/>.
```
