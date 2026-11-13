Backoff strategies for `eb::Retry`.

`eb::backoff::BinaryExponential` is the default strategy: **exponent-truncated
binary exponential backoff with continuous full jitter, inspired by IEEE 802.3**.
It grows a randomized waiting window after each failed attempt. Its default
`Jitter::Continuous` preserves eb's historical distribution; `Jitter::Discrete`
selects integer slot counts as in Ethernet's collision backoff.

The strategy chooses delays. The `eb::Retry` driver controls whether another
attempt happens: total-attempt limits include the initial attempt, `.when`
filters retriable errors, and `.on_retry` supplies a retry hook. The first attempt
is immediate. Success, a non-retriable error, and exhausted limits cause no final
sleep. There is no default attempt limit. An optional wall-clock budget includes
attempt runtime and waiting, but never interrupts a running attempt; it is not a
hard timeout for the operation itself. Success takes precedence over all stop
conditions. After failure, precedence is the retry predicate, attempt limit,
backoff stop, then budget. The budget is rechecked after the retry hook and after
sleeping, before another operation starts; an expired budget returns the retained
error and counts only attempts actually performed.

## Exact algorithm

Number failed attempts starting at `n = 1`: if the initial attempt fails, that is
failure 1, and the next attempt is attempt 2. Within one execution, let `t_i` be
the runtime of failed attempt `i`, excluding retry sleeps.

```text
slot_n = s                                  for Slot::Fixed(s)
slot_n = max(min, (t_1 + ... + t_n) / n)     for Slot::Measured { min }
cap    = min(exponent_cap, 52)
k      = min(n, cap)
W      = slot_n * (2^k - 1)

Jitter::Continuous: D = U * W, U uniform on [0, 1)
Jitter::Discrete:   D = r * slot_n,
                   r uniform integer in {0, ..., 2^k - 1}

delay = min(D, max_delay)                   if max_delay is configured
      = D                                  otherwise
```

`exponent_cap` defaults to 10; continuous jitter is the default. `max_delay` is
an optional hard bound **after** jitter, not a bound on the window before sampling.
For example, clipping a continuous draw to a smaller `max_delay` puts probability
mass at that bound; it does not produce a uniform draw over the shortened window.
The formulas describe ideal arithmetic; see numerical behavior below for finite
precision and saturation. A zero slot or a zero exponent cap produces zero delay.

For a fixed positive slot `s`, default cap 10, no duration clipping, and ordinary
scheduling, the windows and means are:

| Failure `n` | `k` | Window `W` | Continuous delay | Discrete delay | Mean, either mode |
| --- | --- | --- | --- | --- | --- |
| 1 | 1 | `s` | `[0, s)` | `{0, s}` | `0.5s` |
| 2 | 2 | `3s` | `[0, 3s)` | `{0, s, 2s, 3s}` | `1.5s` |
| 3 | 3 | `7s` | `[0, 7s)` | `{0, s, ..., 7s}` | `3.5s` |
| 4 | 4 | `15s` | `[0, 15s)` | `{0, s, ..., 15s}` | `7.5s` |
| 10 and above | 10 | `1023s` | `[0, 1023s)` | `{0, s, ..., 1023s}` | `511.5s` |

Here `3s` means three times the slot, not necessarily three seconds. For a 100 ms
slot, failure 3 gives a 700 ms window and a 350 ms mean in either mode. Continuous
delay is below the window in the ideal model; discrete delay can equal it.
With a measured slot, replace `s` in each row with that failure's `slot_n`.

## Relation to IEEE 802.3

IEEE 802.3's half duplex CSMA/CD backoff uses `k = min(n, 10)` after collision
`n`, then a uniform integer `r` with `0 <= r < 2^k`, waiting `r * slotTime`.
Failure is reported after 16 total unsuccessful transmission attempts, not 16
retries. These rules appear in [IEEE interpretation 1-03/97][ieee-interpretation]
(clause 4.2.3.2.5) and the [IEEE-hosted P802.3as/D0.1 draft][ieee-draft]
(clause 4.2.3.2.5 and Table 4-2).

| Property | IEEE 802.3 half duplex collision backoff | eb default strategy / driver |
| --- | --- | --- |
| Event counted | Collision of the same frame | Failed operation attempt |
| Exponent | `min(n, 10)` | `min(n, exponent_cap)`, default 10 |
| Random choice | Integer `r` in `0..2^k` (upper bound excluded) | Continuous `U` in `[0, 1)` |
| Delay | `r * slotTime` | `U * slot * (2^k - 1)` |
| Slot | Fixed physical timing parameter | `Slot::Fixed` or `Slot::Measured`; CLI defaults to measured |
| Attempt limit | 16 total transmissions per frame | Unlimited unless configured |
| Scheduling | Backoff after collision enforcement (jam) | After failure; optionally `between_starts()` |

For a fixed slot, the two distributions have the same mean, but different
variances. These are mathematical consequences of the distributions above,
before clipping, quantization, or between-starts scheduling:

```text
mean, either mode = s * (2^k - 1) / 2
variance, continuous = s^2 * (2^k - 1)^2 / 12
variance, discrete   = s^2 * (4^k - 1) / 12
```

After the first failure, discrete jitter assigns 50% probability to exactly zero
delay and 50% to one slot. Ideal continuous jitter assigns no probability mass
to exactly zero, although finite random samples and duration rounding can yield
zero in software. It does not choose just zero or one slot.

`BinaryExponential::ieee_802_3(slot)` selects a fixed slot, discrete jitter, and
exponent cap 10. Pair it with `Retry::max_attempts(16)` to match the transmission
attempt limit. This matches the backoff selection rule and count limit, not the
whole Ethernet MAC protocol (carrier sensing, jamming, and interframe spacing).
Adding duration clipping or `between_starts()` changes that timing rule.

```rust
use eb::{backoff::BinaryExponential, Retry};
use std::{num::NonZeroU32, time::Duration};

let policy = Retry::new()
	.with_backoff(BinaryExponential::ieee_802_3(Duration::from_nanos(51_200)))
	.max_attempts(NonZeroU32::new(16).unwrap());
```

The example uses the 51.2 microsecond slot for 10 Mb/s Ethernet.

Ethernet's slot exceeds worst-case round-trip propagation plus maximum jam time,
rather than estimating an average (draft clause 4.2.3.2.3). Table 4-2 gives 512 bit
times at 10/100 Mb/s and 4096 at 1 Gb/s half duplex: respectively 51.2, 5.12, and
4.096 microseconds. Gigabit half duplex uses carrier extension for short frames
(draft clause 4.2.3.3). These bounds support collision detection on a conforming
network; full duplex does not use collision backoff. See the [draft][ieee-draft].

The [IEEE 802.3-2022 listing][ieee-listing] identifies the published standard;
**its exact clause text was not directly verified here**. The comparison above
is supported by the older interpretation and the explicitly unapproved 2005
draft, rather than a claim to have checked the 2022 text.
[Metcalfe and Boggs (1976), section 4.4][metcalfe], supplies historical context:
collision history controls randomized retransmission intervals, and a slot is
related to the maximum collision-detection time. It is not the source for the
later standard's exact constants.

## Other well-known variants

There is no single exponential backoff formula. In the notation below, `U[a, b)`
is a continuous uniform draw, `base` is a time scale, `cap` is a duration bound,
and `prev` is the previous sampled delay. Indexing the first failure varies
between descriptions, so compare formulas as well as names.

[Brooker's AWS article (2015)][aws] distinguishes these policies:

| Policy | Delay formula |
| --- | --- |
| No jitter | `B = min(cap, base * 2^n)` |
| Full Jitter | `U[0, B)` |
| Equal Jitter | `B/2 + U[0, B/2)` |
| Decorrelated Jitter | `min(cap, U[base, 3 * prev))`, starting with `prev = base` |

With a fixed slot, eb's window is
`min(slot * (2^n - 1), slot * (2^exponent_cap - 1))`.
Its continuous mode is similar to AWS Full Jitter, but uses `2^k - 1` rather than
`2^n`. Also distinguish AWS's cap before the Full Jitter draw from eb's optional
`max_delay` clipping after the draw. Equal Jitter retains a deterministic half
window; Decorrelated Jitter depends on the previous sampled delay.

The [gRPC connection backoff protocol][grpc] uses a nominal initial backoff of
1 second, multiplier 1.6, and maximum nominal backoff of 120 seconds. Subsequent
deadlines add uniform jitter of plus or minus 20% of the nominal backoff; the
jittered interval can exceed 120 seconds. Its initial deadline is unjittered,
and connection attempts also have a minimum connection timeout (20 seconds in
the documented parameters). This is a deadline-based connection policy, not
eb's binary full-jitter distribution.

## Choosing a slot

`Slot::Fixed(Duration)` gives a stable time scale. Choose it from the operation's
expected recovery or contention time scale. It is useful when a fast failure
(such as an immediate rejection) should not imply a tiny backoff window.

`Slot::Measured { min }` uses the arithmetic mean of **all failed attempt
runtimes so far in this execution**, including the attempt that just failed,
floored at `min`. It excludes retry sleeps and resets each execution. It is
neither the last runtime alone nor an exponentially smoothed mean. An attempt's
runtime includes time spent awaiting that attempt.

A measured slot adapts delays to the workload's observed time scale. That is the
extent of the analogy to Ethernet's physically chosen slot: a mean runtime is
not a worst-case bound. For contrast, [TCP RTO in RFC 6298, section 2][tcp-rto],
uses smoothed RTT plus a variation allowance (`SRTT + max(G, 4 * RTTVAR)`). eb's
mean does not include such a variance allowance.

Choose a positive `min` if very fast failures would otherwise produce excessive
retry traffic. It floors the slot, not the sampled delay: full jitter can still
choose arbitrarily short waits. Choose `max_delay` separately to bound individual
delays, especially when measured runtimes can grow. Even at exponent cap 10, a
growing measured slot gives a growing window; exponent truncation alone does
**not** impose a fixed wall-clock delay bound. An attempt limit and/or execution
budget controls overall retrying. A running operation needs its own timeout if
its runtime must be bounded.

## Between-starts scheduling

By default, eb waits the entire chosen delay **after** the failed attempt.
The `between_starts()` combinator (CLI `--sync-slots`, [issue #4][issue-4])
instead subtracts that failed attempt's runtime, saturating at zero:

```text
sleep = max(delay - t_n, 0)
nominal time between attempt starts = max(delay, t_n)
```

`between_starts_if(enabled)` selects the same behavior at runtime; `false`
preserves the inner delay. `between_starts()` is equivalent to `between_starts_if(true)`.

This measures the delay from the start of the failed attempt. It cannot undo time
already spent running that attempt. Scheduling and callback overhead can make
actual intervals longer. It is not synchronization to a shared global clock.

With a measured slot, `min <= t_1`, and the default exponent cap, the first
retry's window equals the first attempt's runtime. Its synchronized sleep is
therefore **always zero**, so the next attempt starts immediately. This includes
the CLI's default measured slot with a zero floor. A positive floor larger than
`t_1` can enlarge the first window and permit a nonzero sleep; the immediate-retry
consequence does not hold unconditionally for every `Slot::Measured { min }`.

For continuous jitter, positive window `W`, no clipping, and a given runtime `t`:

```text
P(sleep = 0) = min(t / W, 1)
```

This is a point mass at zero introduced by subtracting runtime. If `W = 0`, sleep
is always zero. If `max_delay <= t`, clipping also makes sleep always zero.
Discrete jitter instead counts integer choices at or below `t / slot`; the
continuous probability formula does not apply to it. Means in the earlier table
are chosen delays, not sleeps after this combinator.

## Numerical behavior

Arithmetic uses `f64`, with saturating conversion to `Duration`. The configured
exponent cap is clamped to 52, the largest exponent for which `2^k - 1` is
exactly representable in `f64`, rather than allowing unbounded exponentiation.
That numerical maximum is separate from the default policy cap of 10. Oversized delays saturate at `max_delay`, if set, or
at `Duration::MAX` rather than panicking from delay arithmetic.

The distribution formulas, means, variances, and probabilities above are ideal
models. Finite random samples, floating-point rounding, nanosecond duration
resolution, saturation, and optional clipping can change endpoints and moments.
In particular, `f64` does not represent every large integer or duration exactly.
A duration bound limits the computed delay, not timer overshoot or attempt runtime.

## Terminology

- **Binary exponential**: the window grows using powers of two.
- **Exponent-truncated**: the exponent stops growing at `exponent_cap`.
- **Duration-capped**: a time bound limits a delay, as with `max_delay`.
- **Attempt-limited**: retrying stops after a configured total number of attempts.

These are separate controls. With a fixed slot, exponent truncation also bounds
the window in time; with a measured slot it need not. Terminology varies:
[Google Cloud's retry guidance][google] also calls a duration-bounded policy
“truncated exponential backoff.” State the formula and the kind of limit rather
than inferring behavior from “truncated” alone.

## References

- [IEEE 802.3 interpretation 1-03/97, CSMA/CD backoff][ieee-interpretation]: clause 4.2.3.2.5 of the 1996 standard.
- [IEEE P802.3as/D0.1, March 2005][ieee-draft]: unapproved draft, clauses 4.2.3.2.3, 4.2.3.2.5, 4.2.3.3, and 4.4.2, Table 4-2.
- [IEEE 802.3-2022 standard listing][ieee-listing]: listing verified, exact clause text not directly verified.
- [Metcalfe and Boggs, *Ethernet: Distributed Packet Switching for Local Computer Networks* (1976)][metcalfe]: section 4.4.
- [Marc Brooker, *Exponential Backoff And Jitter* (2015)][aws].
- [gRPC connection backoff protocol][grpc].
- [RFC 6298, *Computing TCP's Retransmission Timer*][tcp-rto]: section 2.
- [Google Cloud IAM retry strategy][google].

[ieee-interpretation]: https://www.ieee802.org/3/interp/interp-1-0397.pdf
[ieee-draft]: https://www.ieee802.org/3/as/public/0503/4d0_1_CMP.pdf
[ieee-listing]: https://standards.ieee.org/ieee/802.3/10422/
[metcalfe]: https://www.cl.cam.ac.uk/teaching/0708/DigiCommI/metcalfe1976ethernet.pdf
[aws]: https://aws.amazon.com/blogs/architecture/exponential-backoff-and-jitter/
[grpc]: https://github.com/grpc/grpc/blob/master/doc/connection-backoff.md
[tcp-rto]: https://www.rfc-editor.org/rfc/rfc6298
[google]: https://docs.cloud.google.com/iam/docs/retry-strategy
[issue-4]: https://github.com/rye/eb/issues/4

## Custom strategies and reproducible randomness

A `Duration` is itself a constant-delay strategy that never stops on its own;
pair it with a retry limit or budget. Custom strategy clones must own independent
per-execution state; sharing mutable history through `Arc<Mutex<_>>` would leak
history across executions.

Rand 0.10 is a public dependency, available as `eb::rand`. Custom strategies
implement [`Backoff`] with `eb::rand::Rng`; use [`Attempt::new`] to construct
metadata when exercising a strategy directly.

```rust
use eb::{backoff::{Attempt, Backoff}, rand::{rngs::SmallRng, SeedableRng}, Retry};
use std::{num::NonZeroU32, time::Duration};
let mut rng = SmallRng::seed_from_u64(42);
let failed = Attempt::new(NonZeroU32::MIN, Duration::ZERO, Duration::ZERO);
let mut delay = Duration::ZERO;
assert_eq!(delay.next_delay(&failed, &mut rng), Some(Duration::ZERO));
assert_eq!(Retry::new().with_backoff(delay)
	.retry_blocking(|| Ok::<_, ()>(42))
	.rng(SmallRng::seed_from_u64(42)).run(), Ok(42));
```
