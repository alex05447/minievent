//! Thin wrapper around the Windows manual-, auto-reset events and semaphores.
//! Technically provides a portable API, but implemented only for Windows at the moment.
//!
//! See [`event`](https://docs.microsoft.com/en-us/windows/win32/api/synchapi/nf-synchapi-createeventa),
//! [`semaphore`](https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createsemaphorea),
//! [`Wait Functions`](https://docs.microsoft.com/en-us/windows/win32/sync/wait-functions) on MSDN.
//!
//! Run `cargo --doc` for documentation.
//!
//! Uses [`winapi`](https://docs.rs/winapi/0.3.8/winapi/) on Windows.

pub mod event;
pub mod semaphore;
pub mod waitable;

pub use {
    event::{Event, EventError},
    semaphore::{Semaphore, SemaphoreError},
    waitable::{
        max_num_waitables, wait_for_all, wait_for_one, Waitable, WaitableResult, WaitablesResult,
    },
};
