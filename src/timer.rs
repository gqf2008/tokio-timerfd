use std::io;
use std::task::{Context, Poll};
use std::time::Instant;

#[cfg(any(target_os = "linux", target_os = "android"))]
mod imp {
    use super::*;
    use crate::{get_clock, TimerFd};
    use futures_core::ready;
    use std::pin::Pin;
    use std::time::Duration;
    use timerfd::{SetTimeFlags, TimerState};
    use tokio::io::{AsyncRead, ReadBuf};

    pub(crate) struct Timer {
        inner: TimerFd,
    }

    impl Timer {
        pub(crate) fn new() -> io::Result<Self> {
            Ok(Self {
                inner: TimerFd::new(get_clock())?,
            })
        }

        pub(crate) fn arm(&mut self, deadline: Instant) -> io::Result<()> {
            let duration = deadline
                .saturating_duration_since(Instant::now())
                .max(Duration::from_nanos(1));
            self.inner
                .set_state(TimerState::Oneshot(duration), SetTimeFlags::Default);
            Ok(())
        }

        pub(crate) fn disarm(&mut self) {
            self.inner
                .set_state(TimerState::Disarmed, SetTimeFlags::Default);
        }

        pub(crate) fn poll_expired(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            let mut buf = [0u8; 8];
            let mut read_buf = ReadBuf::new(&mut buf);
            ready!(Pin::new(&mut self.inner).poll_read(cx, &mut read_buf))?;
            Poll::Ready(Ok(()))
        }
    }
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
))]
mod imp {
    use super::*;
    use std::convert::TryFrom;
    use std::mem;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::ptr;
    use std::time::Duration;
    use tokio::io::unix::AsyncFd;
    use tokio::io::Interest;

    const TIMER_IDENT: usize = 1;

    pub(crate) struct Timer {
        fd: AsyncFd<OwnedFd>,
    }

    impl Timer {
        pub(crate) fn new() -> io::Result<Self> {
            let raw = unsafe { libc::kqueue() };
            if raw < 0 {
                return Err(io::Error::last_os_error());
            }

            // SAFETY: `raw` is a newly created descriptor that this
            // structure takes ownership of.
            let owned = unsafe { OwnedFd::from_raw_fd(raw) };
            set_cloexec(owned.as_raw_fd());

            Ok(Self {
                fd: AsyncFd::with_interest(owned, Interest::READABLE)?,
            })
        }

        pub(crate) fn arm(&mut self, deadline: Instant) -> io::Result<()> {
            let (data, fflags) =
                duration_to_timer(deadline.saturating_duration_since(Instant::now()));

            // SAFETY: `kevent` is plain data and zero is a valid initial state.
            let mut event: libc::kevent = unsafe { mem::zeroed() };
            event.ident = TIMER_IDENT as _;
            event.filter = libc::EVFILT_TIMER;
            event.flags = libc::EV_ADD | libc::EV_ONESHOT;
            event.fflags = fflags;
            event.data = data as _;

            kqueue_kevent(self.fd.as_raw_fd(), &[event], &mut [], None)?;
            Ok(())
        }

        pub(crate) fn disarm(&mut self) {
            // SAFETY: `kevent` is plain data and zero is a valid initial state.
            let mut event: libc::kevent = unsafe { mem::zeroed() };
            event.ident = TIMER_IDENT as _;
            event.filter = libc::EVFILT_TIMER;
            event.flags = libc::EV_DELETE;

            let _ = kqueue_kevent(self.fd.as_raw_fd(), &[event], &mut [], None);
            self.drain();
        }

        fn drain(&self) {
            let timeout = libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };

            loop {
                // SAFETY: `kevent` is plain data and zero is a valid initial state.
                let mut events: [libc::kevent; 4] = unsafe { mem::zeroed() };
                match kqueue_kevent(self.fd.as_raw_fd(), &[], &mut events, Some(&timeout)) {
                    Ok(0) => break,
                    Ok(count) if count < events.len() => break,
                    Ok(_) => continue,
                    Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        }

        pub(crate) fn poll_expired(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            loop {
                let mut guard = match self.fd.poll_read_ready(cx) {
                    Poll::Ready(Ok(guard)) => guard,
                    Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                    Poll::Pending => return Poll::Pending,
                };

                let timeout = libc::timespec {
                    tv_sec: 0,
                    tv_nsec: 0,
                };
                // SAFETY: `kevent` is plain data and zero is a valid initial state.
                let mut events: [libc::kevent; 4] = unsafe { mem::zeroed() };

                let result = guard.try_io(|inner| {
                    match kqueue_kevent(inner.as_raw_fd(), &[], &mut events, Some(&timeout)) {
                        Ok(0) => Err(io::Error::from(io::ErrorKind::WouldBlock)),
                        Ok(_) => Ok(()),
                        Err(err) => Err(err),
                    }
                });

                match result {
                    Ok(Ok(())) => return Poll::Ready(Ok(())),
                    Ok(Err(err)) if err.kind() == io::ErrorKind::Interrupted => continue,
                    Ok(Err(err)) => return Poll::Ready(Err(err)),
                    Err(_would_block) => continue,
                }
            }
        }
    }

    fn kqueue_kevent(
        fd: libc::c_int,
        changes: &[libc::kevent],
        events: &mut [libc::kevent],
        timeout: Option<&libc::timespec>,
    ) -> io::Result<usize> {
        let timeout = timeout.map_or(ptr::null(), |value| value as *const _);

        // SAFETY: all pointers come from live Rust slices, and `kevent`
        // reads or writes only the number of entries described by the
        // corresponding length arguments.
        let rc = unsafe {
            libc::kevent(
                fd,
                changes.as_ptr(),
                changes.len() as _,
                events.as_mut_ptr(),
                events.len() as _,
                timeout,
            )
        };

        if rc < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(rc as usize)
        }
    }

    fn set_cloexec(fd: libc::c_int) {
        // SAFETY: `fd` is a live descriptor owned by this timer.
        unsafe {
            let flags = libc::fcntl(fd, libc::F_GETFD);
            if flags >= 0 {
                libc::fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC);
            }
        }
    }

    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly"
    ))]
    fn duration_to_timer(duration: Duration) -> (i64, u32) {
        (clamp_i64(duration.as_nanos().max(1)), libc::NOTE_NSECONDS)
    }

    #[cfg(not(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "dragonfly"
    )))]
    fn duration_to_timer(duration: Duration) -> (i64, u32) {
        let milliseconds = duration.as_nanos().div_ceil(1_000_000).max(1);
        (clamp_i64(milliseconds), 0)
    }

    fn clamp_i64(value: u128) -> i64 {
        i64::try_from(value).unwrap_or(i64::MAX)
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use std::convert::TryFrom;
    use std::ffi::c_void;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Mutex;
    use std::task::Waker;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, TRUE};
    use windows_sys::Win32::System::Threading::{
        CancelWaitableTimer, CloseThreadpoolWait, CreateThreadpoolWait, CreateWaitableTimerExW,
        SetThreadpoolWait, SetWaitableTimer, WaitForThreadpoolWaitCallbacks,
        CREATE_WAITABLE_TIMER_HIGH_RESOLUTION, PTP_CALLBACK_INSTANCE, PTP_WAIT, TIMER_ALL_ACCESS,
    };

    struct State {
        fired: AtomicBool,
        waker: Mutex<Option<Waker>>,
    }

    impl State {
        fn new() -> Self {
            Self {
                fired: AtomicBool::new(false),
                waker: Mutex::new(None),
            }
        }

        fn lock_waker(&self) -> std::sync::MutexGuard<'_, Option<Waker>> {
            self.waker
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
        }

        fn reset(&self) {
            self.fired.store(false, Ordering::Release);
            *self.lock_waker() = None;
        }

        fn fire(&self) {
            self.fired.store(true, Ordering::Release);
            if let Some(waker) = self.lock_waker().take() {
                waker.wake();
            }
        }

        fn poll(&self, cx: &mut Context<'_>) -> Poll<()> {
            if self.fired.swap(false, Ordering::Acquire) {
                return Poll::Ready(());
            }

            let mut waker = self.lock_waker();
            if self.fired.swap(false, Ordering::Acquire) {
                return Poll::Ready(());
            }
            if waker
                .as_ref()
                .map_or(true, |current| !current.will_wake(cx.waker()))
            {
                *waker = Some(cx.waker().clone());
            }
            Poll::Pending
        }
    }

    pub(crate) struct Timer {
        handle: HANDLE,
        wait: PTP_WAIT,
        state: Box<State>,
    }

    // SAFETY: Win32 waitable timers and threadpool waits are thread-safe
    // handles. `state` is synchronized with atomics and a mutex, and Drop
    // waits for callbacks before releasing it.
    unsafe impl Send for Timer {}

    impl Timer {
        pub(crate) fn new() -> io::Result<Self> {
            match Self::new_with_flags(CREATE_WAITABLE_TIMER_HIGH_RESOLUTION) {
                Ok(timer) => Ok(timer),
                Err(_) => Self::new_with_flags(0),
            }
        }

        fn new_with_flags(flags: u32) -> io::Result<Self> {
            // SAFETY: null attributes/name are permitted, and the returned handle
            // is checked before use.
            let handle = unsafe {
                CreateWaitableTimerExW(ptr::null(), ptr::null(), flags, TIMER_ALL_ACCESS)
            };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }

            let mut state = Box::new(State::new());
            // SAFETY: state is pinned behind this Box for the lifetime of the
            // timer; Drop waits for callbacks before releasing it.
            let wait = unsafe {
                CreateThreadpoolWait(
                    Some(wait_callback),
                    (&mut *state as *mut State).cast(),
                    ptr::null(),
                )
            };
            if wait == 0 {
                let err = io::Error::last_os_error();
                // SAFETY: handle was created above and is not owned elsewhere.
                unsafe {
                    CloseHandle(handle);
                }
                return Err(err);
            }

            Ok(Self {
                handle,
                wait,
                state,
            })
        }

        pub(crate) fn arm(&mut self, deadline: Instant) -> io::Result<()> {
            self.state.reset();
            let due_time = relative_due_time(deadline.saturating_duration_since(Instant::now()))?;
            // SAFETY: self.handle is a valid timer handle and due_time lives
            // for the duration of the call.
            let ok = unsafe { SetWaitableTimer(self.handle, &due_time, 0, None, ptr::null(), 0) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }

            // SAFETY: both handles are valid for the lifetime of self.
            unsafe {
                SetThreadpoolWait(self.wait, self.handle, ptr::null());
            }
            Ok(())
        }

        pub(crate) fn disarm(&mut self) {
            // SAFETY: both handles are valid; detaching before cancellation
            // prevents a new callback from being queued during shutdown.
            unsafe {
                SetThreadpoolWait(self.wait, ptr::null_mut(), ptr::null());
                CancelWaitableTimer(self.handle);
                WaitForThreadpoolWaitCallbacks(self.wait, TRUE);
            }
            self.state.reset();
        }

        pub(crate) fn poll_expired(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            self.state.poll(cx).map(Ok)
        }
    }

    impl Drop for Timer {
        fn drop(&mut self) {
            // SAFETY: Drop has exclusive access to the handles, and waiting for
            // callbacks before freeing state preserves callback validity.
            unsafe {
                SetThreadpoolWait(self.wait, ptr::null_mut(), ptr::null());
                CancelWaitableTimer(self.handle);
                WaitForThreadpoolWaitCallbacks(self.wait, TRUE);
                CloseThreadpoolWait(self.wait);
                CloseHandle(self.handle);
            }
        }
    }

    fn relative_due_time(duration: Duration) -> io::Result<i64> {
        let ticks = duration.as_nanos().div_ceil(100).max(1);
        let ticks = i64::try_from(ticks).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "timer duration exceeds the Windows waitable-timer range",
            )
        })?;
        Ok(-ticks)
    }

    unsafe extern "system" fn wait_callback(
        _instance: PTP_CALLBACK_INSTANCE,
        context: *mut c_void,
        _wait: PTP_WAIT,
        _wait_result: u32,
    ) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if context.is_null() {
                return;
            }

            // SAFETY: `context` points to the Box<State> owned by Timer. Drop
            // cancels the wait and joins callbacks before releasing that Box.
            unsafe {
                (*(context as *mut State)).fire();
            }
        }));
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::future::poll_fn;

        #[test]
        fn relative_due_time_rounds_up() {
            assert_eq!(relative_due_time(Duration::ZERO).unwrap(), -1);
            assert_eq!(relative_due_time(Duration::from_nanos(1)).unwrap(), -1);
            assert_eq!(relative_due_time(Duration::from_nanos(100)).unwrap(), -1);
            assert_eq!(relative_due_time(Duration::from_nanos(101)).unwrap(), -2);
            assert!(relative_due_time(Duration::MAX).is_err());
        }

        #[tokio::test]
        async fn standard_waitable_timer_works() {
            let mut timer = Timer::new_with_flags(0).unwrap();
            let deadline = Instant::now() + Duration::from_millis(1);
            timer.arm(deadline).unwrap();
            poll_fn(|cx| timer.poll_expired(cx)).await.unwrap();
        }
    }
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    windows,
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
    target_os = "dragonfly"
)))]
mod imp {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use tokio::time::{sleep_until, Instant as TokioInstant, Sleep};

    pub(crate) struct Timer {
        sleep: Option<Pin<Box<Sleep>>>,
    }

    impl Timer {
        pub(crate) fn new() -> io::Result<Self> {
            Ok(Self { sleep: None })
        }

        pub(crate) fn arm(&mut self, deadline: Instant) -> io::Result<()> {
            self.sleep = Some(Box::pin(sleep_until(TokioInstant::from_std(deadline))));
            Ok(())
        }

        pub(crate) fn disarm(&mut self) {
            self.sleep = None;
        }

        pub(crate) fn poll_expired(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            match self.sleep.as_mut() {
                Some(sleep) => sleep.as_mut().poll(cx).map(Ok),
                None => Poll::Pending,
            }
        }
    }
}

pub(crate) struct Timer {
    inner: imp::Timer,
    deadline: Option<Instant>,
    armed: bool,
}

impl Timer {
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self {
            inner: imp::Timer::new()?,
            deadline: None,
            armed: false,
        })
    }

    pub(crate) fn arm(&mut self, deadline: Instant) -> io::Result<()> {
        if self.deadline == Some(deadline) && self.armed {
            return Ok(());
        }

        if self.armed {
            self.inner.disarm();
            self.armed = false;
        }
        self.deadline = Some(deadline);

        if deadline > Instant::now() {
            if let Err(err) = self.inner.arm(deadline) {
                self.deadline = None;
                return Err(err);
            }
            self.armed = true;
        }
        Ok(())
    }

    pub(crate) fn disarm(&mut self) {
        if self.armed {
            self.inner.disarm();
        }
        self.deadline = None;
        self.armed = false;
    }

    pub(crate) fn poll_expired(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.deadline {
            None => Poll::Pending,
            Some(deadline) if Instant::now() >= deadline => {
                self.deadline = None;
                self.armed = false;
                Poll::Ready(Ok(()))
            }
            Some(_) => match self.inner.poll_expired(cx) {
                Poll::Ready(Ok(())) => {
                    self.deadline = None;
                    self.armed = false;
                    Poll::Ready(Ok(()))
                }
                other => other,
            },
        }
    }
}
