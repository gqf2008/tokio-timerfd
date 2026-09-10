tokio-timerfd
=============

OS-backed high-resolution timers for Tokio.

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
    tokio_timerfd::sleep(Duration::from_millis(10)).await.unwrap();
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

The APIs provide higher timer resolution where the operating system supports it,
but actual wakeup latency remains dependent on scheduling and system load. They
do not provide hard real-time guarantees.

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
