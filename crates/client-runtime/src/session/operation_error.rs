use super::*;
use std::{error::Error, fmt};

impl fmt::Display for QueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Time(error) => error.fmt(f),
            Self::Protocol(error) => error.fmt(f),
            other => write!(f, "client queue rejected: {other:?}"),
        }
    }
}

impl Error for QueueError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Time(error) => Some(error),
            Self::Protocol(error) => Some(error),
            _ => None,
        }
    }
}
