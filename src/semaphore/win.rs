use {
    crate::{SemaphoreError, Waitable, WaitableResult},
    std::{ffi::CString, io, ptr, time::Duration},
    winapi::{
        shared::{minwindef::TRUE, winerror::WAIT_TIMEOUT},
        um::{
            handleapi::CloseHandle,
            synchapi::{ReleaseSemaphore, WaitForSingleObject},
            winbase::{CreateSemaphoreA, INFINITE, WAIT_OBJECT_0},
            winnt::HANDLE,
        },
    },
};

/// Waitable semaphore wrapper.
/// See [`semaphore`](https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createsemaphorea) on MSDN.
///
/// The semaphore is signaled when the internal counter is above `0`.
/// The internal counter is initialized to `init_count` by [`new`].
/// When [`increment`] is called with `count` argument, at most `count` threads
/// will wake up and the counter will be decremented for each woken up thread.
///
/// Closes the owned OS event handle when dropped.
///
/// [`new`]: #method.new
/// [`increment`]: #method.increment
pub struct Semaphore {
    handle: HANDLE,
}

impl Semaphore {
    /// Creates a new semaphore (or tries to reuse based on `name`).
    ///
    /// `init_count` - initializes the internal counter value. Clamped to be less or equal to `max_count`.
    /// `max_count` - determines the maximum value the internal counter may be incremented to
    /// before the call to [`increment`] fails.
    /// `name` - see the [`docs`](https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createsemaphorea).
    ///
    /// # Errors
    ///
    /// Returns an error if the OS event creation failed, if `name` was invalid - e.g. contained nul bytes.
    ///
    /// [`increment`]: #method.increment
    pub fn new(
        mut init_count: usize,
        max_count: usize,
        name: Option<&str>,
    ) -> Result<Semaphore, SemaphoreError> {
        use SemaphoreError::*;

        init_count = init_count.min(max_count);

        let name = if let Some(name) = name {
            if name.len() > 0 {
                CString::new(name).map_err(|_| InvalidName)?.as_ptr()
            } else {
                ptr::null_mut()
            }
        } else {
            ptr::null_mut()
        };

        let handle =
            unsafe { CreateSemaphoreA(ptr::null_mut(), init_count as i32, max_count as i32, name) };

        if handle.is_null() {
            Err(FailedToCreate(io::Error::last_os_error()))
        } else {
            Ok(Semaphore { handle })
        }
    }

    /// Increments the semaphore's internal counter by `count`.
    /// Up to `count` waiting threads may be woken up.
    ///
    /// Fails if the internal counter value would overflow its maximum value
    /// as determined by `max_count` in [`new`] if `count` was to be added to it.
    ///
    /// On success returns the previous counter value.
    ///
    /// [`new`]: #method.new
    pub fn increment(&self, count: usize) -> Result<usize, SemaphoreError> {
        let mut prev_count: i32 = 0;

        let result =
            unsafe { ReleaseSemaphore(self.handle, count as i32, &mut prev_count as *mut i32) };

        if result == TRUE {
            Ok(prev_count as usize)
        } else {
            Err(SemaphoreError::FailedToIncrement(io::Error::last_os_error()))
        }
    }

    /// Increments the semaphore's internal counter by `1`.
    /// At most one waiting thread may be woken up.
    ///
    /// Fails if the internal counter value would overflow its maximum value
    /// as determined by `max_count` in [`new`] if `1` was to be added to it.
    ///
    /// On success returns the previous counter value.
    ///
    /// [`new`]: #method.new
    pub fn increment_one(&self) -> Result<usize, SemaphoreError> {
        self.increment(1)
    }

    fn wait_impl(&self, ms: u32) -> Result<WaitableResult, SemaphoreError> {
        let result = unsafe { WaitForSingleObject(self.handle, ms) };

        match result {
            WAIT_OBJECT_0 => Ok(WaitableResult::Signaled),
            WAIT_TIMEOUT => Ok(WaitableResult::Timeout),
            _ => Err(SemaphoreError::FailedToWait(io::Error::last_os_error())),
        }
    }
}

impl Drop for Semaphore {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

unsafe impl Send for Semaphore {}
unsafe impl Sync for Semaphore {}

impl Waitable for Semaphore {
    /// Blocks the thread until the semaphore is [`incremented`] or the duration `d` expires.
    ///
    /// # Errors
    ///
    /// Returns an error if the OS function fails.
    ///
    /// [`incremented`]: struct.Semaphore.html#method.increment
    fn wait(&self, d: Duration) -> Result<WaitableResult, ()> {
        let ms = d.as_millis();
        debug_assert!(ms <= std::u32::MAX as u128);
        let ms = ms as u32;

        self.wait_impl(ms).map_err(|_| ())
    }

    /// Blocks the thread until the semaphore is [`incremented`].
    ///
    /// Because at most one thread is woken up when the semaphore is [`incremented`],
    /// there's no guarantee any given thread will wake up when there are multiple
    /// threads waiting for one semaphore.
    ///
    /// # Errors
    ///
    /// Returns an error if the OS function fails.
    ///
    /// [`incremented`]: struct.Semaphore.html#method.increment
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
        crate::wait_for_all,
        std::{sync::Arc, thread, time::Instant},
    };

    #[test]
    fn signaled_method() {
        let s = Semaphore::new(1, 1, None).unwrap(); // Signaled.

        let res = s.wait(Duration::from_secs(1_000_000)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        let res = s.wait(Duration::from_millis(1)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Timeout);

        s.increment_one().unwrap(); // Signaled again.

        let res = s.wait(Duration::from_secs(1_000_000)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        s.increment_one().unwrap(); // Signaled again.

        s.increment_one().err().unwrap(); // Must have failed.
    }

    #[test]
    fn signaled_free_function() {
        let s = Semaphore::new(1, 1, None).unwrap(); // Signaled.
        let w = [&s as _];

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        let res = wait_for_all(&w, Duration::from_millis(1)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Timeout);

        s.increment_one().unwrap(); // Signaled again.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        s.increment_one().unwrap(); // Signaled again.

        s.increment_one().err().unwrap(); // Must have failed.
    }

    #[test]
    fn unsignaled_method() {
        let s = Semaphore::new(0, 1, None).unwrap(); // Not signaled.

        let res = s.wait(Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);

        s.increment_one().unwrap(); // Now signaled.

        let res = s.wait(Duration::from_secs(1_000_000)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        s.increment_one().unwrap(); // Signaled again.

        let res = s.wait(Duration::from_secs(1_000_000)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        s.increment(2).err().unwrap(); // Must have failed.
    }

    #[test]
    fn unsignaled_free_function() {
        let s = Semaphore::new(0, 1, None).unwrap(); // Not signaled.
        let w = [&s as _];

        let res = wait_for_all(&w, Duration::from_millis(1)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Timeout);

        s.increment_one().unwrap(); // Now signaled.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        s.increment_one().unwrap(); // Signaled again.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)).unwrap(); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        s.increment(2).err().unwrap(); // Must have failed.
    }

    #[test]
    fn thread_signal() {
        let s = Arc::new(Semaphore::new(0, 2, None).unwrap()); // Not signaled.
        let s_clone_1 = s.clone();
        let s_clone_2 = s.clone();

        let t_1 = thread::spawn(move || {
            let now = Instant::now();
            let res = s_clone_1.wait(Duration::from_secs(1_000_000)).unwrap();
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        let t_2 = thread::spawn(move || {
            let now = Instant::now();
            let res = s_clone_2.wait(Duration::from_secs(1_000_000)).unwrap();
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        // Wait for a second.
        thread::sleep(Duration::from_secs(1));

        s.increment_one().unwrap();

        // One of the threads has exited, the other is still waiting.

        thread::sleep(Duration::from_millis(1_000));

        s.increment_one().unwrap();

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

        let res = s.wait(Duration::from_millis(1)).unwrap();
        assert!(res == WaitableResult::Timeout);
    }
}
