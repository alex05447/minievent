mod error;

#[cfg(windows)]
mod win;

pub use error::EventError;

#[cfg(windows)]
pub use win::Event;
