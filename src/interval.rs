use crate::timer::Timer;
use futures_core::{ready, Stream};
use std::convert::TryFrom;
use std::io::Error as IoError;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

/// A stream that yields at fixed intervals.
///
/// Missed ticks are skipped: after a scheduling delay, the next deadline is
/// advanced to the next period boundary at or after the current time.
pub struct Interval {
    timer: Timer,
    next: Instant,
    duration: Duration,
}

impl Interval {
    /// Creates a new `Interval` whose first tick is at `at`, followed by
    /// ticks every `duration`.
    ///
    /// # Panics
    ///
    /// Panics if `duration` is zero.
    pub fn new(at: Instant, duration: Duration) -> Result<Interval, IoError> {
        assert!(
            duration > Duration::new(0, 0),
            "`duration` must be non-zero."
        );
        Ok(Interval {
            timer: Timer::new()?,
            next: at,
            duration,
        })
    }

    /// Creates a new `Interval` whose first tick is `duration` from now.
    ///
    /// # Panics
    ///
    /// Panics if `duration` is zero.
    pub fn new_interval(duration: Duration) -> Result<Interval, IoError> {
        Self::new(Instant::now() + duration, duration)
    }
}

impl Stream for Interval {
    type Item = Result<(), IoError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        while Instant::now() < self.next {
            let next = self.next;
            self.timer.arm(next)?;
            ready!(self.timer.poll_expired(cx))?;
        }

        let fired = self.next;
        self.next = next_deadline(fired, Instant::now(), self.duration);
        Poll::Ready(Some(Ok(())))
    }
}

fn next_deadline(fired: Instant, now: Instant, duration: Duration) -> Instant {
    let next = fired + duration;
    if now < next {
        return next;
    }

    let elapsed = now.saturating_duration_since(fired).as_nanos();
    let periods = elapsed / duration.as_nanos().max(1) + 1;
    let periods = u32::try_from(periods).unwrap_or(u32::MAX);
    fired + duration.saturating_mul(periods)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    #[tokio::test]
    async fn interval_works() {
        let mut interval = Interval::new_interval(Duration::from_millis(1)).unwrap();

        let start = Instant::now();
        for _ in 0..5 {
            interval.next().await.unwrap().unwrap();
        }
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn long_interval_works() {
        let mut interval = Interval::new_interval(Duration::from_millis(20)).unwrap();

        for _ in 0..3 {
            let before = Instant::now();
            interval.next().await.unwrap().unwrap();
            let elapsed = before.elapsed();
            assert!(elapsed >= Duration::from_millis(10));
            assert!(elapsed < Duration::from_secs(1));
        }
    }

    #[test]
    fn missed_ticks_advance_to_next_boundary() {
        let fired = Instant::now();
        let duration = Duration::from_millis(100);
        let now = fired + Duration::from_millis(250);
        assert_eq!(
            next_deadline(fired, now, duration),
            fired + Duration::from_millis(300)
        );
    }
}
