use crate::timer::Timer;
use futures_core::ready;
use std::future::Future;
use std::io::Error as IoError;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

/// A future that completes at a specified instant.
///
/// `Delay` uses the operating system's native timer facility where available
/// and returns an error if the timer cannot be created or polled.
pub struct Delay {
    timer: Timer,
    deadline: Instant,
    initialized: bool,
}

impl Delay {
    /// Creates a new `Delay` that completes at `deadline`.
    pub fn new(deadline: Instant) -> Result<Self, IoError> {
        Ok(Delay {
            timer: Timer::new()?,
            deadline,
            initialized: false,
        })
    }

    /// Returns the instant at which the future will complete.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Returns `true` once `deadline` has been reached.
    pub fn is_elapsed(&self) -> bool {
        self.deadline <= Instant::now()
    }

    /// Resets the delay to a new deadline.
    pub fn reset(&mut self, deadline: Instant) {
        self.timer.disarm();
        self.deadline = deadline;
        self.initialized = false;
    }
}

impl Future for Delay {
    type Output = Result<(), IoError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.initialized {
            let deadline = self.deadline;
            self.timer.arm(deadline)?;
            self.initialized = true;
        }
        ready!(self.timer.poll_expired(cx))?;
        Poll::Ready(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn delay_zero_duration() -> Result<(), std::io::Error> {
        let now = Instant::now();
        let delay = Delay::new(Instant::now())?;
        delay.await?;
        assert!(now.elapsed() < Duration::from_secs(1));
        Ok(())
    }

    #[tokio::test]
    async fn delay_works() {
        let now = Instant::now();
        let delay = Delay::new(now + Duration::from_millis(10)).unwrap();
        delay.await.unwrap();
        assert!(now.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn sleep_overflow_is_rejected() {
        assert!(crate::try_sleep(Duration::MAX).is_err());
    }

    #[tokio::test]
    async fn delay_reset_rearms_timer() -> Result<(), std::io::Error> {
        let start = Instant::now();
        let mut delay = Delay::new(start + Duration::from_secs(60))?;
        delay.reset(start + Duration::from_millis(10));
        delay.await?;
        assert!(start.elapsed() < Duration::from_secs(1));
        Ok(())
    }

    #[tokio::test]
    async fn dropping_pending_delay_is_safe() -> Result<(), std::io::Error> {
        let delay = Delay::new(Instant::now() + Duration::from_secs(60))?;
        drop(delay);
        Ok(())
    }

    #[tokio::test]
    async fn try_sleep_works() -> Result<(), std::io::Error> {
        crate::try_sleep(Duration::from_millis(1))?.await
    }

    #[tokio::test]
    async fn delay_reports_elapsed_state() -> Result<(), std::io::Error> {
        let mut delay = Delay::new(Instant::now() + Duration::from_millis(10))?;
        assert!(!delay.is_elapsed());

        delay.reset(Instant::now() - Duration::from_millis(1));
        assert!(delay.is_elapsed());

        delay.reset(Instant::now() + Duration::from_millis(10));
        assert!(!delay.is_elapsed());
        Ok(())
    }
}
