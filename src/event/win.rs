use std::ptr;
use std::ffi::CString;
use std::time::Duration;

use winapi::shared::minwindef::FALSE;
use winapi::shared::winerror::WAIT_TIMEOUT;
use winapi::um::winbase::{ WAIT_OBJECT_0, INFINITE };
use winapi::um::winnt::HANDLE;
use winapi::um::synchapi::{ CreateEventA, SetEvent, ResetEvent, WaitForSingleObject };
use winapi::um::handleapi::CloseHandle;

use crate::waitable::{ Waitable, WaitableResult };

/// Waitable event wrapper.
/// See [`event`](https://docs.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createeventa) on MSDN.
///
/// Auto event: gets reset when one awaiting thread is woken up.
/// You must call [`set`] once for each awaiting thread.
///
/// Manual event: stays set/reset when [`set`] / [`reset`] is called on it.
///
/// [`set`]: #method.set
/// [`reset`]: #method.reset
pub struct Event {
    handle :HANDLE,
}

impl Event {
    /// Creates a new auto reset event.
    /// `signaled` - sets the initial state of the event.
    ///
    /// # Panics
    ///
    /// Panics if the OS event creation fails.
    pub fn new_auto(signaled :bool, name :Option<&str>) -> Event {
        Event::new(false, signaled, name)
    }

    /// Creates a new manual reset event.
    /// `signaled` - sets the initial state of the event.
    ///
    /// # Panics
    ///
    /// Panics if the OS event creation fails.
    pub fn new_manual(signaled :bool, name :Option<&str>) -> Event {
        Event::new(true, signaled, name)
    }

    /// Signals the event.
    /// Auto event: at most one waiting thread will be woken up.
    /// Manual event: stays signalled until it is [`reset`].
    /// # Panics
    ///
    /// Panics if the OS function fails.
    ///
    /// [`reset`]: #method.reset
    pub fn set(&self) {
        let result = unsafe {
            SetEvent(self.handle)
        };

        assert!(result != FALSE);
    }

    /// Resets the event.
    ///
    /// # Panics
    ///
    /// Panics if the OS function fails.
    ///
    pub fn reset(&self) {
        let result = unsafe {
            ResetEvent(self.handle)
        };

        assert!(result != FALSE);
    }

    fn new(manual :bool, signaled :bool, name :Option<&str>) -> Event {
        let manual = if manual { 1 } else { 0 };
        let signaled = if signaled { 1 } else { 0 };

        let name = if name.is_some() {
            if name.unwrap().len() > 0 {
                CString::new(name.unwrap()).expect("Invalid event name.").as_ptr()
            } else {
                ptr::null_mut()
            }
        } else {
            ptr::null_mut()
        };

        let handle = unsafe { CreateEventA(
            ptr::null_mut(),
            manual,
            signaled,
            name
        ) };

        if handle.is_null() {
            panic!("Event creation failed.");
        }

        Event {
            handle
        }
    }

    fn wait_internal(&self, ms :u32) -> WaitableResult {
        let result = unsafe {
            WaitForSingleObject(self.handle, ms)
        };

        match result {
            WAIT_OBJECT_0 => WaitableResult::Signaled,
            WAIT_TIMEOUT => WaitableResult::Timeout,
            _ => panic!("Event wait error."),
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
    /// # Panics
    ///
    /// Panics if the OS function fails or if the event was abandoned.
    ///
    /// [`set`]: #method.set
    fn wait(&self, d :Duration) -> WaitableResult {
        let ms = d.as_millis();
        debug_assert!(ms <= std::u32::MAX as u128);
        let ms = ms as u32;

        self.wait_internal(ms)
    }

    /// Blocks the thread until the event is [`set`].
    /// Auto event: because at most one thread is woken up when the event is [`set`],
    /// there's no guarantee any given thread will wake up when there are multiple
    /// threads waiting for one event.
    ///
    /// # Panics
    ///
    /// Panics if the OS function fails or if the event was abandoned.
    ///
    /// [`set`]: #method.set
    fn wait_infinite(&self) {
        self.wait_internal(INFINITE);
    }

    fn handle(&self) -> *mut () {
        self.handle as *mut ()
    }
}

#[cfg(test)]
use std::{ thread, sync::Arc, time::Instant };

#[cfg(test)]
mod tests {
    use super::*;

    use crate::waitable::{ WaitablesResult, wait_for_one, wait_for_all };

    #[test]
    fn manual_reset_signaled_method() {
        let e = Event::new_manual(true, None); // Signaled.

        let res = e.wait(Duration::from_secs(1_000_000)); // Still signaled.
        assert!(res == WaitableResult::Signaled);

        let res = e.wait(Duration::from_secs(1_000_000)); // And still signaled.
        assert!(res == WaitableResult::Signaled);

        e.reset(); // Not anymore.

        let res = e.wait(Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);

        e.set(); // Signaled again.

        let res = e.wait(Duration::from_secs(1_000_000));
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn manual_reset_signaled_free_function() {
        let e = Event::new_manual(true, None); // Signaled.
        let w = [&e as &Waitable];

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)); // Still signaled.
        assert!(res == WaitableResult::Signaled);

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)); // And still signaled.
        assert!(res == WaitableResult::Signaled);

        e.reset(); // Not anymore.

        let res = wait_for_all(&w, Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);

        e.set(); // Signaled again.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000));
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn manual_reset_signaled_free_function_multiple() {
        let e0 = Event::new_manual(true, None);
        let e1 = Event::new_manual(true, None);
        let w = [&e0 as &Waitable, &e1 as &Waitable]; // Signaled.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000)); // Still signaled.
        assert!(res == WaitableResult::Signaled);

        let res = wait_for_one(&w, Duration::from_secs(1_000_000)); // And still signaled.
        assert!(res == WaitablesResult::OneSignaled(0) ||
                res == WaitablesResult::OneSignaled(1) );

        e0.reset(); // Not anymore.

        let res = wait_for_all(&w, Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);

        let res = wait_for_one(&w, Duration::from_secs(1_000_000));
        assert!(res == WaitablesResult::OneSignaled(1));

        e1.reset(); // Both not set.

        let res = wait_for_all(&w, Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);

        let res = wait_for_one(&w, Duration::from_millis(1));
        assert!(res == WaitablesResult::Timeout);

        e0.set(); // Only one set.

        let res = wait_for_all(&w, Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);

        let res = wait_for_one(&w, Duration::from_secs(1_000_000));
        assert!(res == WaitablesResult::OneSignaled(0));

        e1.set(); // Both signaled again.

        let res = wait_for_all(&w, Duration::from_secs(1_000_000));
        assert!(res == WaitableResult::Signaled);

        let res = wait_for_one(&w, Duration::from_secs(1_000_000));
        assert!(res == WaitablesResult::OneSignaled(0) ||
                res == WaitablesResult::OneSignaled(1) );
    }

    #[test]
    fn manual_reset_unsignaled_method() {
        let e = Event::new_manual(false, None); // Not signaled.

        let res = e.wait(Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);

        e.set(); // Now signaled.

        let res = e.wait(Duration::from_secs(1_000_000)); // Still signaled.
        assert!(res == WaitableResult::Signaled);

        let res = e.wait(Duration::from_secs(1_000_000)); // And still signaled.
        assert!(res == WaitableResult::Signaled);

        e.reset(); // Not anymore.

        let res = e.wait(Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);

        e.set(); // Signaled again.

        let res = e.wait(Duration::from_secs(1_000_000));
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn auto_reset_signaled() {
        let e = Event::new_auto(true, None); // Signaled.

        let res = e.wait(Duration::from_secs(1_000_000)); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        let res = e.wait(Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);

        e.set(); // Signaled again.

        let res = e.wait(Duration::from_secs(1_000_000));
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn auto_reset_unsignaled() {
        let e = Event::new_auto(false, None); // Not signaled.

        let res = e.wait(Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);

        e.set(); // Now signaled.

        let res = e.wait(Duration::from_secs(1_000_000)); // Not signaled.
        assert!(res == WaitableResult::Signaled);

        e.set(); // Signaled again.

        let res = e.wait(Duration::from_secs(1_000_000));
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn manual_thread_signal() {
        let e = Arc::new(Event::new_manual(false, None));
        let e_clone_1 = e.clone();
        let e_clone_2 = e.clone(); // Not signaled.

        let t_1 = thread::spawn(move || {
            let now = Instant::now();
            let res = e_clone_1.wait(Duration::from_secs(1_000_000));
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        let t_2 = thread::spawn(move || {
            let now = Instant::now();
            let res = e_clone_2.wait(Duration::from_secs(1_000_000));
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        // Wait for a second.
        thread::sleep(Duration::from_secs(1));

        e.set();

        // Both threads must have exited at about the same time.

        let res_1 = t_1.join().unwrap();

        assert!(res_1.0 == WaitableResult::Signaled);
        assert!(res_1.1.as_millis() >= 500);

        let res_2 = t_2.join().unwrap();

        assert!(res_2.0 == WaitableResult::Signaled);
        assert!(res_2.1.as_millis() >= 500);

        // Still signaled.

        let res = e.wait(Duration::from_secs(1_000_000));
        assert!(res == WaitableResult::Signaled);
    }

    #[test]
    fn auto_thread_signal() {
        let e = Arc::new(Event::new_auto(false, None));
        let e_clone_1 = e.clone();
        let e_clone_2 = e.clone(); // Not signaled.

        let t_1 = thread::spawn(move || {
            let now = Instant::now();
            let res = e_clone_1.wait(Duration::from_secs(1_000_000));
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        let t_2 = thread::spawn(move || {
            let now = Instant::now();
            let res = e_clone_2.wait(Duration::from_secs(1_000_000));
            let elapsed = now.elapsed();
            (res, elapsed)
        });

        // Wait for a second.
        thread::sleep(Duration::from_secs(1));

        e.set();

        // One of the threads has exited, the other is still waiting.

        thread::sleep(Duration::from_millis(1_000));

        e.set();

        // Now both have exited.

        let res_1 = t_1.join().unwrap();

        assert!(res_1.0 == WaitableResult::Signaled);
        assert!(res_1.1.as_millis() >= 500);

        let res_2 = t_2.join().unwrap();

        assert!(res_2.0 == WaitableResult::Signaled);
        assert!(res_2.1.as_millis() >= 500);

        if res_1.1.as_millis() > res_2.1.as_millis() {
            assert!( res_1.1.as_millis() - res_2.1.as_millis() >= 500 );
        } else {
            assert!( res_2.1.as_millis() - res_1.1.as_millis() >= 500 );
        }

        // Not signaled.

        let res = e.wait(Duration::from_millis(1));
        assert!(res == WaitableResult::Timeout);
    }
}