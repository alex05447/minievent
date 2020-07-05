use std::{
    error::Error,
    fmt::{Display, Formatter},
    io,
};

#[derive(Debug)]
pub enum EventError {
    FailedToCreate(io::Error),
    InvalidName,
    FailedToSet(io::Error),
    FailedToReset(io::Error),
    FailedToWait(io::Error),
}

impl Error for EventError {}

impl Display for EventError {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        use EventError::*;

        match self {
            FailedToCreate(err) => write!(f, "failed to create the event: {}", err),
            InvalidName => "invalid event name".fmt(f),
            FailedToSet(err) => write!(f, "failed to set the event: {}", err),
            FailedToReset(err) => write!(f, "failed to reset the event: {}", err),
            FailedToWait(err) => write!(f, "failed to wait on the event: {}", err),
        }
    }
}
