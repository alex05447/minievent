use {
    crate::{EventError, Waitable, WaitableResult},
    std::{ffi::CString, io, ptr, time::Duration},
    winapi::{
        shared::{minwindef::FALSE, winerror::WAIT_TIMEOUT},
        um::{
            handleapi::CloseHandle,
            synchapi::{CreateEventA, ResetEvent, SetEvent, WaitForSingleObject},
            winbase::{INFINITE, WAIT_OBJECT_0},
            winnt::HANDLE,
        },
    },
};

/// Waitable event wrapper.
/// See [`event`](https://docs.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createeventa) on MSDN.
///
/// Auto event: gets reset when one awaiting thread is woken up.
/// You must call [`set`] once for each awaiting thread.
///
/// Manual event: stays set/reset when [`set`] / [`reset`] is called on it.
///
/// Closes the owned OS event handle when dropped.
///
/// [`set`]: #method.set
/// [`reset`]: #method.reset
pub struct Event {
    handle: HANDLE,
}

impl Event {
    /// Creates a new auto reset event (or tries to reuse based on `name`).
    ///
    /// `set` - gives the initial state of the event.
    /// `name` - see the [`docs`](https://docs.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createeventw).
    ///
    /// # Errors
    ///
    /// Returns an error if the OS event creation failed, or if `name` was invalid - e.g. contained nul bytes.
    pub fn new_auto<'n, N: Into<Option<&'n str>>>(set: bool, name: N) -> Result<Event, EventError> {
        Event::new(false, set, name.into())
    }

    /// Creates a new manual reset event (or tries to reuse based on `name`).
    ///
    /// `set` - gives the initial state of the event.
    /// `name` - see the [`docs`](https://docs.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createeventa).
    ///
    /// # Errors
    ///
    /// Returns an error if the OS event creation failed, or if `name` was invalid - e.g. contained nul bytes.
    pub fn new_manual<'n, N: Into<Option<&'n str>>>(
        set: bool,
        name: N,
    ) -> Result<Event, EventError> {
        Event::new(true, set, name.into())
    }

    /// Sets / signals the event.
    ///
    /// Auto event: at most one waiting thread will be woken up.
    /// Manual event: stays set / signaled until it is [`reset`].
    ///
    /// [`reset`]: #method.reset
    pub fn set(&self) -> Result<(), EventError> {
        let result = unsafe { SetEvent(self.handle) };

        if result == FALSE {
            Err(EventError::FailedToSet(io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }

    /// Resets the manual reset event.
    pub fn reset(&self) -> Result<(), EventError> {
        let result = unsafe { ResetEvent(self.handle) };

        if result == FALSE {
            Err(EventError::FailedToReset(io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }

    fn new(manual: bool, set: bool, name: Option<&str>) -> Result<Event, EventError> {
        use EventError::*;

        let manual = if manual { 1 } else { 0 };
        let set = if set { 1 } else { 0 };

        let name = if let Some(name) = name {
            if name.len() > 0 {
                CString::new(name).map_err(|_| InvalidName)?.as_ptr()
            } else {
                ptr::null_mut()
            }
        } else {
            ptr::null_mut()
        };

        let handle = unsafe { CreateEventA(ptr::null_mut(), manual, set, name) };

        if handle.is_null() {
            Err(FailedToCreate(io::Error::last_os_error()))
        } else {
            Ok(Event { handle })
        }
    }

    fn wait_impl(&self, ms: u32) -> Result<WaitableResult, EventError> {
        let result = unsafe { WaitForSingleObject(self.handle, ms) };

        match result {
            WAIT_OBJECT_0 => Ok(WaitableResult::Signaled),
            WAIT_TIMEOUT => Ok(WaitableResult::Timeout),
            _ => Err(EventError::FailedToWait(io::Error::last_os_error())),
        }
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

unsafe impl Send for Event {}
unsafe impl Sync for Event {}

impl Waitable for Event {
    /// Blocks the thread until the event is [`set`] or the duration `d` expires.
    ///
    /// # Errors
    ///
    /// Returns an error if the OS function fails or if the event was abandoned.
    ///
    /// [`set`]: struct.Event.html#method.set
    fn wait(&self, d: Duration) -> Result<WaitableResult, ()> {
        let ms = d.as_millis();
        debug_assert!(ms <= std::u32::MAX as u128);
        let ms = ms as u32;

        self.wait_impl(ms).map_err(|_| ())
    }

    /// Blocks the thread until the event is [`set`].
    ///
    /// Auto event: because at most one thread is woken up when the event is [`set`],
    /// there's no guarantee any given thread will wake up when there are multiple
    /// threads waiting for one event.
    ///
    /// # Errors
    ///
    /// Returns an error if the OS function fails or if the event was abandoned.
    ///
    /// [`set`]: struct.Event.html#method.set
    fn wait_infinite(&self) -> Result<(), ()> {
        self.wait_impl(INFINITE).map(|_| ()).map_err(|_| ())
    }

    /// Returns the raw handle / pointer to the waitable's OS object.
    fn handle(&self) -> *mut () {
        self.handle as *mut ()
    }
}

#[cfg(test)]
mod tests {
    use {
        super::*,
        crate::{wait_for_all, wait_for_one, WaitablesResult},
        std::{sync::Arc, thread, time::Instant},
    };

    #[test]
    fn manual_reset_signaled_method() {
        let e = Event::new_manual(true, None).unwrap(); // Signaled.

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap(); // Still signaled.
        assert!(res == WaitableResult::Signaled);

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap(); // And still signaled.
        assert!(res == WaitableResult::Signaled);

        e.reset().unwrap(); // Not anymore.

        let res = e.wait(Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);

        e.set().unwrap(); // Signaled again.

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap();
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn manual_reset_signaled_free_function() {
        let e = Event::new_manual(true, None).unwrap(); // Signaled.
        let w = [&e as _];

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)).unwrap(); // Still signaled.
        assert!(res == WaitableResult::Signaled);

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)).unwrap(); // And still signaled.
        assert!(res == WaitableResult::Signaled);

        e.reset().unwrap(); // Not anymore.

        let res = wait_for_all(&w, Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);

        e.set().unwrap(); // Signaled again.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)).unwrap();
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn manual_reset_signaled_free_function_multiple() {
        let e0 = Event::new_manual(true, None).unwrap(); // Signaled.
        let e1 = Event::new_manual(true, None).unwrap(); // Signaled.
        let w = [&e0 as _, &e1 as _]; // Signaled.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)).unwrap(); // Still signaled.
        assert!(res == WaitableResult::Signaled);

        let res = wait_for_one(&w, Duration::from_secs(1_000_000)).unwrap(); // And still signaled.
        assert!(res == WaitablesResult::OneSignaled(0) || res == WaitablesResult::OneSignaled(1));

        e0.reset().unwrap(); // One not signaled.

        let res = wait_for_all(&w, Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);

        let res = wait_for_one(&w, Duration::from_secs(1_000_000)).unwrap();
        assert!(res == WaitablesResult::OneSignaled(1));

        e1.reset().unwrap(); // Both not signaled.

        let res = wait_for_all(&w, Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);

        let res = wait_for_one(&w, Duration::from_millis(1)).unwrap();
        assert!(res == WaitablesResult::Timeout);

        e0.set().unwrap(); // Only one signaled.

        let res = wait_for_all(&w, Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);

        let res = wait_for_one(&w, Duration::from_secs(1_000_000)).unwrap();
        assert!(res == WaitablesResult::OneSignaled(0));

        e1.set().unwrap(); // Both signaled again.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)).unwrap();
        assert!(res == WaitableResult::Signaled);

        let res = wait_for_one(&w, Duration::from_secs(1_000_000)).unwrap();
        assert!(res == WaitablesResult::OneSignaled(0) || res == WaitablesResult::OneSignaled(1));
    }

    #[test]
    fn manual_reset_unsignaled_method() {
        let e = Event::new_manual(false, None).unwrap(); // Not signaled.

        let res = e.wait(Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);

        e.set().unwrap(); // Now signaled.

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap(); // Still signaled.
        assert!(res == WaitableResult::Signaled);

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap(); // And still signaled.
        assert!(res == WaitableResult::Signaled);

        e.reset().unwrap(); // Not anymore.

        let res = e.wait(Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);

        e.set().unwrap(); // Signaled again.

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap();
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn auto_reset_signaled() {
        let e = Event::new_auto(true, None).unwrap(); // Signaled.

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        let res = e.wait(Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);

        e.set().unwrap(); // Signaled again.

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap();
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn auto_reset_unsignaled() {
        let e = Event::new_auto(false, None).unwrap(); // Not signaled.

        let res = e.wait(Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);

        e.set().unwrap(); // Now signaled.

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        e.set().unwrap(); // Signaled again.

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap();
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn manual_thread_signal() {
        let e = Arc::new(Event::new_manual(false, None).unwrap());
        let e_clone_1 = e.clone();
        let e_clone_2 = e.clone(); // Not signaled.

        let t_1 = thread::spawn(move || {
            let now = Instant::now();
            let res = e_clone_1.wait(Duration::from_secs(1_000_000)).unwrap();
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        let t_2 = thread::spawn(move || {
            let now = Instant::now();
            let res = e_clone_2.wait(Duration::from_secs(1_000_000)).unwrap();
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        // Wait for a second.
        thread::sleep(Duration::from_secs(1));

        e.set().unwrap();

        // Both threads must have exited at about the same time.

        let res_1 = t_1.join().unwrap();

        assert!(res_1.0 == WaitableResult::Signaled);
        assert!(res_1.1.as_millis() >= 500);

        let res_2 = t_2.join().unwrap();

        assert!(res_2.0 == WaitableResult::Signaled);
        assert!(res_2.1.as_millis() >= 500);

        // Still signaled.

        let res = e.wait(Duration::from_secs(1_000_000)).unwrap();
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn auto_thread_signal() {
        let e = Arc::new(Event::new_auto(false, None).unwrap());
        let e_clone_1 = e.clone();
        let e_clone_2 = e.clone(); // Not signaled.

        let t_1 = thread::spawn(move || {
            let now = Instant::now();
            let res = e_clone_1.wait(Duration::from_secs(1_000_000)).unwrap();
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        let t_2 = thread::spawn(move || {
            let now = Instant::now();
            let res = e_clone_2.wait(Duration::from_secs(1_000_000)).unwrap();
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        // Wait for a second.
        thread::sleep(Duration::from_secs(1));

        e.set().unwrap();

        // One of the threads has exited, the other is still waiting.

        thread::sleep(Duration::from_millis(1_000));

        e.set().unwrap();

        // Now both have exited.

        let res_1 = t_1.join().unwrap();

        assert!(res_1.0 == WaitableResult::Signaled);
        assert!(res_1.1.as_millis() >= 500);

        let res_2 = t_2.join().unwrap();

        assert!(res_2.0 == WaitableResult::Signaled);
        assert!(res_2.1.as_millis() >= 500);

        if res_1.1.as_millis() > res_2.1.as_millis() {
            assert!(res_1.1.as_millis() - res_2.1.as_millis() >= 500);
        } else {
            assert!(res_2.1.as_millis() - res_1.1.as_millis() >= 500);
        }

        // Not signaled.

        let res = e.wait(Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);
    }
}
