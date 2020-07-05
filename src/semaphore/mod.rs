mod error;

#[cfg(windows)]
mod win;

pub use error::SemaphoreError;

#[cfg(windows)]
pub use win::Semaphore;
