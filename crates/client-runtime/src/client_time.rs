//! Host-supplied monotonic samples, separate from UTC and resource clocks.
use std::{
    error::Error,
    fmt,
    time::{Duration, Instant},
};

/// One sample from the host's monotonic clock. Construction reads no OS clock.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ClientTime(Instant);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientTimeError {
    Backward,
    Overflow,
}

impl ClientTimeError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Backward => "client_runtime.time_backward",
            Self::Overflow => "client_runtime.time_overflow",
        }
    }
}

impl fmt::Display for ClientTimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Backward => "client monotonic time moved backwards",
            Self::Overflow => "client monotonic deadline or elapsed time overflowed",
        })
    }
}
impl Error for ClientTimeError {}

impl From<minimium_transport::client::ClientTransportTimeError> for ClientTimeError {
    fn from(error: minimium_transport::client::ClientTransportTimeError) -> Self {
        match error {
            minimium_transport::client::ClientTransportTimeError::Backward => Self::Backward,
            minimium_transport::client::ClientTransportTimeError::Overflow => Self::Overflow,
        }
    }
}

impl ClientTime {
    pub const fn new(sample: Instant) -> Self {
        Self(sample)
    }
    pub const fn as_instant(self) -> Instant {
        self.0
    }

    pub fn checked_add(self, duration: Duration) -> Result<Self, ClientTimeError> {
        self.0
            .checked_add(duration)
            .map(Self)
            .ok_or(ClientTimeError::Overflow)
    }

    pub fn checked_duration_since(self, earlier: Self) -> Result<Duration, ClientTimeError> {
        self.0
            .checked_duration_since(earlier.0)
            .ok_or(ClientTimeError::Backward)
    }

    /// Pure preflight. The longest logical preparation interval must fit before
    /// an operation can mutate a lifecycle owner or enqueue output.
    pub fn validate_after(self, previous: Self) -> Result<(), ClientTimeError> {
        self.checked_duration_since(previous)?;
        self.checked_add(Duration::from_secs(300))?;
        Ok(())
    }

    pub(crate) fn elapsed_since(self, earlier: Self) -> Duration {
        self.checked_duration_since(earlier)
            .expect("logical samples and stored timestamps were prevalidated")
    }
}
