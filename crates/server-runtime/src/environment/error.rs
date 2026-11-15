use minimium_environment::time::TimeError;
use std::{error::Error, fmt, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EnvironmentClockErrorKind {
    SystemClock,
    TimeRange,
    RevisionExhausted,
    InvalidConfiguration,
    InvalidSavedState,
    InvalidSample,
    RandomSource,
}

#[derive(Debug)]
pub(crate) struct EnvironmentClockError {
    kind: EnvironmentClockErrorKind,
    detail: String,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl EnvironmentClockError {
    pub(crate) fn new(kind: EnvironmentClockErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
            source: None,
        }
    }

    pub(crate) fn caused_by<E>(
        kind: EnvironmentClockErrorKind,
        detail: impl Into<String>,
        source: E,
    ) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self {
            kind,
            detail: detail.into(),
            source: Some(Box::new(source)),
        }
    }

    pub(crate) fn from_time(error: TimeError) -> Self {
        let kind = match error {
            TimeError::RevisionExhausted => EnvironmentClockErrorKind::RevisionExhausted,
            TimeError::OutOfSupportedRange(_) => EnvironmentClockErrorKind::TimeRange,
            _ => EnvironmentClockErrorKind::InvalidSample,
        };
        Self::caused_by(kind, error.to_string(), error)
    }

    pub(crate) fn fault(kind: EnvironmentClockErrorKind, code: &'static str) -> Self {
        Self::new(kind, code)
    }

    pub(crate) const fn code(&self) -> &'static str {
        match self.kind {
            EnvironmentClockErrorKind::SystemClock => "environment.system_clock_unavailable",
            EnvironmentClockErrorKind::TimeRange => "environment.time_range_exhausted",
            EnvironmentClockErrorKind::RevisionExhausted => "environment.clock_revision_exhausted",
            EnvironmentClockErrorKind::InvalidConfiguration => {
                "environment.clock_configuration_invalid"
            }
            EnvironmentClockErrorKind::InvalidSavedState => "environment.saved_clock_invalid",
            EnvironmentClockErrorKind::InvalidSample => "environment.clock_sample_invalid",
            EnvironmentClockErrorKind::RandomSource => "environment.session_nonce_failed",
        }
    }
}

impl fmt::Display for EnvironmentClockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.detail)
    }
}

impl Error for EnvironmentClockError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

impl From<EnvironmentClockError> for io::Error {
    fn from(error: EnvironmentClockError) -> Self {
        io::Error::other(error)
    }
}
