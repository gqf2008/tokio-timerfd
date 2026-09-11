#![deny(missing_docs)]
#![forbid(unsafe_op_in_unsafe_fn)]

//! OS-backed timers for Tokio.
//!
//! The active backends are:
//!
//! * Linux and Android: [`timerfd`](https://man7.org/linux/man-pages/man2/timerfd_create.2.html).
//! * Windows: a high-resolution waitable timer when supported, with a
//!   standard waitable-timer fallback on older Windows versions.
//! * macOS, iOS, and BSD: `kqueue` with `EVFILT_TIMER`; NetBSD and OpenBSD
//!   currently fall back to millisecond granularity.
//! * Other platforms: a Tokio timer-wheel fallback.
//!
//! "High resolution" describes the operating-system timer facility and the
//! granularity it can represent; it is not a promise that a task is polled at
//! the requested deadline. Resolution is only the timer's representable step:
//!
//! * Deadline error is the difference between the actual and requested
//!   wakeup. It includes timer delivery, OS scheduling, task wakeup, and load.
//! * Jitter is variation in that error between wakeups. Low jitter does not
//!   imply low absolute latency.
//! * Drift is accumulated error over repeated periods. `Interval` re-arms
//!   against an absolute schedule to avoid unbounded drift, but it cannot
//!   remove scheduler-induced latency.
//!
//! These APIs do not provide hard real-time guarantees.
//!
//! * [`Delay`]: a future that completes at a specified instant.
//! * [`Interval`]: a stream that yields at fixed intervals.
//! * [`DelayQueue`]: a queue that multiplexes many deadlines over one timer.
//! * [`sleep`] and [`try_sleep`]: convenience functions that create a [`Delay`].
//!
//! Timer constructors must be called while a Tokio runtime is active.
//!
//! On Linux, the `realtime` and `boottime` features select an alternative
//! `timerfd` clock. With neither feature enabled, the monotonic clock is used.
//! These features have no effect on other platforms.

use std::io;
use std::time::{Duration, Instant};

#[cfg(all(feature = "realtime", feature = "boottime"))]
compile_error!("features `realtime` and `boottime` are mutually exclusive");

mod delay;
mod delay_queue;
mod interval;
mod timer;

pub use delay::Delay;
pub use delay_queue::{DelayQueue, Expired, Key};
pub use interval::Interval;

#[cfg(any(target_os = "linux", target_os = "android"))]
pub use timerfd::ClockId;

#[cfg(any(target_os = "linux", target_os = "android"))]
use std::io::{Error, Result};
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::os::unix::io::{AsRawFd, RawFd};
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::pin::Pin;
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::task::{Context, Poll};

#[cfg(any(target_os = "linux", target_os = "android"))]
use futures_core::ready;
#[cfg(any(target_os = "linux", target_os = "android"))]
use timerfd::{SetTimeFlags, TimerFd as InnerTimerFd, TimerState};
#[cfg(any(target_os = "linux", target_os = "android"))]
use tokio::io::unix::AsyncFd;
#[cfg(any(target_os = "linux", target_os = "android"))]
use tokio::io::{AsyncRead, Interest, ReadBuf};

/// A low-level Linux `timerfd` integrated with the Tokio reactor.
///
/// This type is available only on Linux and Android. Most applications
/// should use [`Delay`] or [`Interval`] instead.
#[cfg(any(target_os = "linux", target_os = "android"))]
pub struct TimerFd(AsyncFd<InnerTimerFd>);

#[cfg(any(target_os = "linux", target_os = "android"))]
impl TimerFd {
    /// Creates and registers a non-blocking `timerfd` using `clock`.
    pub fn new(clock: ClockId) -> io::Result<Self> {
        let fd = InnerTimerFd::new_custom(clock, true, true)?;
        let inner = AsyncFd::with_interest(fd, Interest::READABLE)?;
        Ok(TimerFd(inner))
    }

    fn set_state(&mut self, state: TimerState, flags: SetTimeFlags) -> TimerState {
        self.0.get_mut().set_state(state, flags)
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
impl AsRawFd for TimerFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn read_u64(fd: RawFd) -> Result<u64> {
    loop {
        let mut buf = [0u8; 8];
        let rv = unsafe {
            // SAFETY: `fd` is owned by the TimerFd and `buf` has exactly
            // eight bytes, the size required by timerfd reads.
            libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len())
        };

        if rv == buf.len() as isize {
            return Ok(u64::from_ne_bytes(buf));
        }
        if rv < 0 {
            let err = Error::last_os_error();
            if err.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(err);
        }

        return Err(Error::new(
            io::ErrorKind::UnexpectedEof,
            "timerfd returned an incomplete expiration count",
        ));
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
impl AsyncRead for TimerFd {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<()>> {
        if buf.remaining() < 8 {
            return Poll::Ready(Err(Error::new(
                io::ErrorKind::InvalidInput,
                "TimerFd requires at least 8 bytes of buffer space",
            )));
        }

        let inner = self.as_mut();
        let fd = inner.0.as_raw_fd();

        loop {
            let mut guard = ready!(inner.0.poll_read_ready(cx))?;
            match guard.try_io(|_| read_u64(fd)) {
                Ok(res) => {
                    let num = res?;
                    buf.put_slice(&num.to_ne_bytes());
                    break;
                }
                Err(_) => continue,
            }
        }
        Poll::Ready(Ok(()))
    }
}

/// Creates a future that completes in `duration` from now.
///
/// This convenience function panics if the operating-system timer cannot be
/// created. Use [`try_sleep`] to handle that error explicitly.
pub fn sleep(duration: Duration) -> Delay {
    try_sleep(duration).expect("can't create delay")
}

/// Tries to create a future that completes in `duration` from now.
pub fn try_sleep(duration: Duration) -> io::Result<Delay> {
    let deadline = Instant::now().checked_add(duration).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "sleep duration exceeds the supported clock range",
        )
    })?;
    Delay::new(deadline)
}

#[cfg(all(test, any(target_os = "linux", target_os = "android")))]
mod timerfd_tests {
    use super::*;
    use std::future::poll_fn;

    #[tokio::test]
    async fn short_read_buffer_is_rejected() {
        let mut timer = TimerFd::new(ClockId::Monotonic).unwrap();
        let mut buffer = [0_u8; 1];
        let mut read_buffer = ReadBuf::new(&mut buffer);
        let err = poll_fn(|cx| Pin::new(&mut timer).poll_read(cx, &mut read_buffer))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[cfg(feature = "realtime")]
fn get_clock() -> ClockId {
    ClockId::Realtime
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[cfg(feature = "boottime")]
fn get_clock() -> ClockId {
    ClockId::Boottime
}

#[cfg(any(target_os = "linux", target_os = "android"))]
#[cfg(not(any(feature = "boottime", feature = "realtime")))]
fn get_clock() -> ClockId {
    ClockId::Monotonic
}
