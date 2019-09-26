use std::ffi::CString;
use std::ptr;
use std::time::Duration;

use winapi::shared::minwindef::TRUE;
use winapi::shared::winerror::WAIT_TIMEOUT;
use winapi::um::handleapi::CloseHandle;
use winapi::um::synchapi::{ReleaseSemaphore, WaitForSingleObject};
use winapi::um::winbase::CreateSemaphoreA;
use winapi::um::winbase::{INFINITE, WAIT_OBJECT_0};
use winapi::um::winnt::HANDLE;

use crate::waitable::{Waitable, WaitableResult};

/// Waitable semaphore wrapper.
/// See [`semaphore`](https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createsemaphorea) on MSDN.
///
/// The semaphore is signaled when the internal counter is above `0`.
/// The internal counter is initialized to `init_count` by [`new`].
/// When [`increment`] is called with `count` argument, at most `count` threads
/// will wake up and the counter will be decremented for each woken up thread.
///
/// [`new`]: #method.new
/// [`increment`]: #method.increment
pub struct Semaphore {
    handle: HANDLE,
}

impl Semaphore {
    /// Creates a new semaphore.
    /// `init_count` - initializes the internal counter value.
    /// `max_count` - determines the maximum value the internal counter may be incremented to
    /// before the call to [`increment`] fails.
    ///
    /// # Panics
    ///
    /// Panics if the OS event creation fails.
    ///
    /// [`increment`]: #method.increment
    pub fn new(init_count: usize, max_count: usize, name: Option<&str>) -> Semaphore {
        assert!(
            init_count <= max_count,
            "`init_count` must be less or equal to `max_count`."
        );

        let name = if name.is_some() {
            if name.unwrap().len() > 0 {
                CString::new(name.unwrap())
                    .expect("Invalid semaphore name.")
                    .as_ptr()
            } else {
                ptr::null_mut()
            }
        } else {
            ptr::null_mut()
        };

        let handle =
            unsafe { CreateSemaphoreA(ptr::null_mut(), init_count as i32, max_count as i32, name) };

        if handle.is_null() {
            panic!("Semaphore creation failed.");
        }

        Semaphore { handle }
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
    pub fn increment(&self, count: usize) -> Result<usize, ()> {
        let mut prev_count: i32 = 0;

        let result =
            unsafe { ReleaseSemaphore(self.handle, count as i32, &mut prev_count as *mut i32) };

        if result == TRUE {
            Ok(prev_count as usize)
        } else {
            Err(())
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
    pub fn increment_one(&self) -> Result<usize, ()> {
        self.increment(1)
    }

    fn wait_internal(&self, ms: u32) -> WaitableResult {
        let result = unsafe { WaitForSingleObject(self.handle, ms) };

        match result {
            WAIT_OBJECT_0 => WaitableResult::Signaled,
            WAIT_TIMEOUT => WaitableResult::Timeout,
            _ => panic!("Semaphore wait error."),
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
    /// Blocks the thread until the semaphore is [`increment`]'ed or the duration `d` expires.
    ///
    /// # Panics
    ///
    /// Panics if the OS function fails or if the semaphore was abandoned.
    ///
    /// [`increment`]: #method.increment
    fn wait(&self, d: Duration) -> WaitableResult {
        let ms = d.as_millis();
        debug_assert!(ms <= std::u32::MAX as u128);
        let ms = ms as u32;

        self.wait_internal(ms)
    }

    /// Blocks the thread until the semaphore is [`increment`]'ed.
    ///
    /// Because at most one thread is woken up when the semaphore is [`increment`]'ed,
    /// there's no guarantee any given thread will wake up when there are multiple
    /// threads waiting for one semaphore.
    ///
    /// # Panics
    ///
    /// Panics if the OS function fails or if the semaphore was abandoned.
    ///
    /// [`increment`]: #method.increment
    fn wait_infinite(&self) {
        self.wait_internal(INFINITE);
    }

    fn handle(&self) -> *mut () {
        self.handle as *mut ()
    }
}

#[cfg(test)]
use std::{sync::Arc, thread, time::Instant};

#[cfg(test)]
mod tests {
    use super::*;

    use crate::waitable::wait_for_all;

    #[test]
    fn signaled_method() {
        let s = Semaphore::new(1, 1, None); // Signaled.

        let res = s.wait(Duration::from_secs(1_000_000)); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        let res = s.wait(Duration::from_millis(1)); // Not signaled.
        assert!(res == WaitableResult::Timeout);

        s.increment_one().unwrap(); // Signaled again.

        let res = s.wait(Duration::from_secs(1_000_000)); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        s.increment_one().unwrap(); // Signaled again.

        let error = s.increment_one(); // Must have failed.

        assert!(error.is_err());
    }

    #[test]
    fn signaled_free_function() {
        let s = Semaphore::new(1, 1, None); // Signaled.
        let w = [&s as &Waitable];

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        let res = wait_for_all(&w, Duration::from_millis(1)); // Not signaled.
        assert!(res == WaitableResult::Timeout);

        s.increment_one().unwrap(); // Signaled again.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        s.increment_one().unwrap(); // Not signaled.

        let error = s.increment_one(); // Signaled again.

        assert!(error.is_err()); // Must have failed.
    }

    #[test]
    fn unsignaled_method() {
        let s = Semaphore::new(0, 1, None); // Not signaled.

        let res = s.wait(Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);

        s.increment_one().unwrap(); // Now signaled.

        let res = s.wait(Duration::from_secs(1_000_000)); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        s.increment_one().unwrap(); // Signaled again.

        let res = s.wait(Duration::from_secs(1_000_000)); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        let error = s.increment(2); // Must have failed.

        assert!(error.is_err());
    }

    #[test]
    fn unsignaled_free_function() {
        let s = Semaphore::new(0, 1, None); // Not signaled.
        let w = [&s as &Waitable];

        let res = wait_for_all(&w, Duration::from_millis(1)); // Not signaled.
        assert!(res == WaitableResult::Timeout);

        s.increment_one().unwrap(); // Now signaled.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        s.increment_one().unwrap(); // Signaled again.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        let error = s.increment(2); // Must have failed.

        assert!(error.is_err());
    }

    #[test]
    fn thread_signal() {
        let s = Arc::new(Semaphore::new(0, 2, None)); // Not signaled.
        let s_clone_1 = s.clone();
        let s_clone_2 = s.clone();

        let t_1 = thread::spawn(move || {
            let now = Instant::now();
            let res = s_clone_1.wait(Duration::from_secs(1_000_000));
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        let t_2 = thread::spawn(move || {
            let now = Instant::now();
            let res = s_clone_2.wait(Duration::from_secs(1_000_000));
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

        let res = s.wait(Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);
    }
}
