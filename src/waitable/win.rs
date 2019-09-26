use std::time::Duration;

use winapi::shared::winerror::WAIT_TIMEOUT;
use winapi::um::synchapi::WaitForMultipleObjectsEx;
use winapi::um::winbase::WAIT_OBJECT_0;
use winapi::um::winnt::{HANDLE, MAXIMUM_WAIT_OBJECTS};

use super::{Waitable, WaitableResult, WaitablesResult};

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
/// # Panics
///
/// Panics if the OS function fails.
/// Panics if the len of `waitables` exceeds the value returned by [`max_num_waitables`].
///
/// [`max_num_waitables`]: fn.max_num_waitables.html
pub fn wait_for_all(waitables: &[&dyn Waitable], d: Duration) -> WaitableResult {
    match wait_for_events_internal(waitables, d, true) {
        WaitablesResult::AllSignaled => WaitableResult::Signaled,
        WaitablesResult::Timeout => WaitableResult::Timeout,
        _ => panic!("Wait error."),
    }
}

/// Blocks the thread until at least one of the waitables are signaled or the duration `d` expires.
/// Maximum number of waitables is platform-dependant and returned by [`max_num_waitables`].
///
/// # Panics
///
/// Panics if the OS function fails.
/// Panics if the len of `waitables` exceeds the value returned by [`max_num_waitables`].
///
/// [`max_num_waitables`]: fn.max_num_waitables.html
pub fn wait_for_one(waitables: &[&dyn Waitable], d: Duration) -> WaitablesResult {
    wait_for_events_internal(waitables, d, false)
}

fn wait_for_events_internal(
    waitables: &[&dyn Waitable],
    d: Duration,
    wait_for_all: bool,
) -> WaitablesResult {
    let num_waitables = waitables.len();

    assert!(
        num_waitables <= max_num_waitables(),
        "Too many simultaneous waitables (max {} supported).",
        max_num_waitables()
    );

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
            WaitablesResult::AllSignaled
        } else {
            WaitablesResult::OneSignaled(result as usize)
        }
    } else if result == WAIT_TIMEOUT {
        WaitablesResult::Timeout
    } else {
        panic!("Wait error.");
    }
}
