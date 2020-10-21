use {
    crate::{WaitableResult, WaitablesResult},
    std::time::Duration,
    winapi::{
        shared::winerror::WAIT_TIMEOUT,
        um::{
            synchapi::WaitForMultipleObjectsEx,
            winbase::WAIT_OBJECT_0,
            winnt::{HANDLE, MAXIMUM_WAIT_OBJECTS},
        },
    },
};

/// Platform-specific waitable object extension trait.
pub trait WaitableExt {
    /// Returns the raw handle / pointer to the waitable's OS object.
    fn handle(&self) -> *mut ();
}

/// Returns the platfrom-specific maximum number of waitables
/// accepted by the call to [`wait_for_all`] / [`wait_for_one`].
///
/// On Windows, `MAXIMUM_WAIT_OBJECTS` is 64.
///
/// [`wait_for_all`]: fn.wait_for_all.html
/// [`wait_for_one`]: fn.wait_for_one.html
pub fn max_num_waitables() -> usize {
    return MAXIMUM_WAIT_OBJECTS as usize;
}

/// Blocks the thread until all waitables are signaled or the duration `d` expires.
/// Maximum number of waitables is platform-dependant and returned by [`max_num_waitables`].
///
/// # Errors
///
/// Returns an error if the OS function fails.
/// Returns an error if the len of `waitables` exceeds the value returned by [`max_num_waitables`].
///
/// [`max_num_waitables`]: fn.max_num_waitables.html
pub fn wait_for_all(waitables: &[&dyn WaitableExt], d: Duration) -> Result<WaitableResult, ()> {
    match wait_for_waitables_impl(waitables, d, true) {
        Ok(WaitablesResult::AllSignaled) => Ok(WaitableResult::Signaled),
        Ok(WaitablesResult::Timeout) => Ok(WaitableResult::Timeout),
        _ => Err(()),
    }
}

/// Blocks the thread until at least one of the waitables are signaled or the duration `d` expires.
/// Maximum number of waitables is platform-dependant and returned by [`max_num_waitables`].
///
/// # Errors
///
/// Returns an error if the OS function fails.
/// Returns an error if the len of `waitables` exceeds the value returned by [`max_num_waitables`].
///
/// [`max_num_waitables`]: fn.max_num_waitables.html
pub fn wait_for_one(waitables: &[&dyn WaitableExt], d: Duration) -> Result<WaitablesResult, ()> {
    wait_for_waitables_impl(waitables, d, false)
}

fn wait_for_waitables_impl(
    waitables: &[&dyn WaitableExt],
    d: Duration,
    wait_for_all: bool,
) -> Result<WaitablesResult, ()> {
    let num_waitables = waitables.len();

    if num_waitables > max_num_waitables() {
        return Err(());
    }

    let mut handles = [0 as HANDLE; MAXIMUM_WAIT_OBJECTS as usize];

    for (idx, waitable) in waitables.iter().enumerate() {
        handles[idx] = waitable.handle() as HANDLE;
    }

    let ms = d.as_millis();
    debug_assert!(ms <= std::u32::MAX as u128);
    let ms = ms as u32;

    let handles = handles.as_ptr();

    let result = unsafe {
        let wait_for_all = if wait_for_all { 1 } else { 0 };
        WaitForMultipleObjectsEx(num_waitables as u32, handles, wait_for_all, ms, 0)
    };

    if result < (WAIT_OBJECT_0 + num_waitables as u32) {
        if wait_for_all {
            Ok(WaitablesResult::AllSignaled)
        } else {
            Ok(WaitablesResult::OneSignaled(result as usize))
        }
    } else if result == WAIT_TIMEOUT {
        Ok(WaitablesResult::Timeout)
    } else {
        Err(())
    }
}
