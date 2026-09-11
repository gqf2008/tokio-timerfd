tokio-timerd
=============

[![CI](https://github.com/gqf2008/tokio-timerfd/actions/workflows/ci.yml/badge.svg)](https://github.com/gqf2008/tokio-timerfd/actions/workflows/ci.yml)

OS-backed high-resolution timers for Tokio.

This repository is maintained independently at [gqf2008/tokio-timerfd](https://github.com/gqf2008/tokio-timerfd).

| Platform | Backend |
| --- | --- |
| Linux / Android | `timerfd` integrated with Tokio `AsyncFd` |
| Windows 10 1803+ / Server 2019+ | high-resolution waitable timer |
| Older Windows | standard waitable-timer fallback |
| macOS / iOS / FreeBSD / DragonFly BSD | `kqueue` `EVFILT_TIMER` with nanosecond units |
| NetBSD / OpenBSD | `kqueue` `EVFILT_TIMER` with millisecond units |
| Other platforms | `tokio::time` fallback |

The crate provides:

- `Delay`, a future that completes at a specified instant.
- `Interval`, a stream that yields at fixed periods.
- `DelayQueue`, a queue that multiplexes many deadlines over one timer.
- `sleep(duration)` and `try_sleep(duration)` for creating a `Delay`.
- `TimerFd`, a Linux/Android `timerfd` handle that implements `AsyncRead`.

Example:

```rust
use std::time::Duration;

#[tokio::main]
async fn main() {
    tokio_timerd::sleep(Duration::from_millis(10)).await.unwrap();
}
```

On Linux, `realtime` and `boottime` select an alternative `timerfd` clock. The
default is the monotonic clock. These features are mutually exclusive and have
no effect on other platforms.

## Platform support

| Platform | Verification status |
| --- | --- |
| Linux | Runtime tests in CI |
| Android | `timerfd` backend; no CI target yet |
| Windows | Runtime tests locally and in CI |
| macOS | Runtime tests in CI; iOS compile-checked |
| FreeBSD | Runtime tests in CI |
| NetBSD | Runtime tests in CI |
| OpenBSD | Runtime tests in CI |
| DragonFly BSD | Best effort until runtime coverage exists |

## Precision semantics

"High resolution" describes the timer facility and the granularity exposed by the
OS backend. It does not mean "wakes exactly on time" and it is not a hard
real-time guarantee. Resolution is the size of the timer's step; accuracy is how
close a wakeup actually is to the requested deadline.

| Term | Meaning in this crate |
| --- | --- |
| Resolution | The finest increment the backend can represent. It is a property of the timer interface, not a bound on wakeup error. |
| Deadline error | `actual wakeup - requested deadline` (positive means late). It includes timer delivery, OS scheduling, Tokio task wakeup, and system load. |
| Jitter | Variation in deadline error between wakeups. Low jitter does not imply low absolute latency. |
| Drift | Accumulated deadline error over repeated periods. `Interval` advances an absolute schedule instead of adding the period after each wakeup, preventing one delayed tick from shifting every later tick. |

For example, Windows can represent sub-millisecond deadlines while one-shot
wakeups may still occur about 0.5-1 ms late. An `Interval` can show much smaller
tick-to-tick deltas because a fixed phase offset is present in every tick and
cancels out of the delta. See [BENCHMARK.md](BENCHMARK.md) for methodology and
results.

## Benchmarks

Measure end-to-end `Delay` error and `Interval` jitter with:

```sh
cargo bench --bench timer_latency
```

See [BENCHMARK.md](BENCHMARK.md) for methodology and sample results. Benchmarks
are hardware- and load-sensitive and are not CI gates.

## Changelog

See [CHANGELOG.md](CHANGELOG.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development and platform-testing requirements.

## Security

See [SECURITY.md](SECURITY.md) for vulnerability reporting.

## License

Licensed under the [MIT License](LICENSE).
