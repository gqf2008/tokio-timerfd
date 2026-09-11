# Changelog

## [0.3.3] - 2026-09-11

### Changed

- Clarified resolution, absolute latency, jitter, and drift semantics in the README and crate documentation.

## [0.3.2] - 2026-09-11

### Changed

- Added timer-construction measurements to the benchmark.
- Documented the measured 0.5-1ms absolute scheduling floor on Windows.

## [0.3.1] - 2026-09-11

### Changed

- Renamed the crate to `tokio-timerd` for independent crates.io distribution.

## [0.3.0] - 2026-09-11

### Added

- Windows high-resolution waitable timer backend using `CreateWaitableTimerExW`
  and `CreateThreadpoolWait`, with a standard waitable-timer fallback.
- macOS, iOS, and BSD `kqueue`/`EVFILT_TIMER` backend.
- Portable Tokio timer-wheel fallback for other platforms.
- Reimplemented `DelayQueue` on top of one native timer and a min-heap.
- `try_sleep` for fallible delay construction.
- Delay and interval latency/jitter benchmark.
- Linux, Windows, macOS, and BSD CI coverage.

### Changed

- `TimerFd`, `ClockId`, and the `realtime`/`boottime` features are now available
  only on Linux and Android. Other platforms use the cross-platform timer API.
- Missed interval ticks are skipped and the next deadline is advanced to the
  next period boundary.
- Short `TimerFd` reads now return `InvalidInput` instead of panicking.

### Fixed

- `Delay::is_elapsed()` no longer reports future deadlines as elapsed.
- Partial `timerfd` reads and interrupted reads are handled explicitly.
- Enabling both `realtime` and `boottime` is rejected at compile time.
