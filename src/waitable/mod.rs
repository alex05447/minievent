use std::time::Duration;

/// Result of waiting on a single waitable, or multiple waitables if all must be siganled.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WaitableResult {
    /// The waitable was signaled / all waitables were signaled.
    Signaled,
    /// The timeout duration elapsed before the waitable was signaled / all waitables were signaled.
    Timeout,
}

/// Result of waiting on multiple waitables.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WaitablesResult {
    /// One of the waitables was signaled.
    /// Contains the index of the signaled waitable.
    OneSignaled(usize),
    /// All of the waitables were signaled.
    AllSignaled,
    /// The timeout duration elapsed before any waitable was signaled.
    Timeout,
}

/// Waitable object trait.
pub trait Waitable {
    /// Blocks the thread until the waitable is signaled or the duration `d` expires.
    fn wait(&self, d: Duration) -> Result<WaitableResult, ()>;

    /// Blocks the thread until the waitable is signaled.
    fn wait_infinite(&self) -> Result<(), ()>;

    /// Returns the raw handle / pointer to the waitable's OS object.
    fn handle(&self) -> *mut ();
}

#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use win::{max_num_waitables, wait_for_all, wait_for_one};
