use std::{
    error::Error,
    fmt::{Display, Formatter},
    io,
};

#[derive(Debug)]
pub enum SemaphoreError {
    FailedToCreate(io::Error),
    InvalidName,
    FailedToIncrement(io::Error),
    FailedToWait(io::Error),
}

impl Error for SemaphoreError {}

impl Display for SemaphoreError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        use SemaphoreError::*;

        match self {
            FailedToCreate(err) => write!(f, "failed to create the semaphore: {}", err),
            InvalidName => "invalid semaphore name".fmt(f),
            FailedToIncrement(err) => write!(f, "failed to increment the semaphore: {}", err),
            FailedToWait(err) => write!(f, "failed to wait on the semaphore: {}", err),
        }
    }
}
