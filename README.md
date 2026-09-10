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
- `sleep(duration)`, a convenience function for creating a `Delay`.
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

The APIs provide higher timer resolution where the operating system supports it,
but actual wakeup latency remains dependent on scheduling and system load. They
do not provide hard real-time guarantees.
