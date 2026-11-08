use std::{error::Error, fmt, io};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AssetIoErrorKind {
    NotFound,
    InvalidFormat,
    UnsupportedVersion,
    DigestMismatch,
    LimitExceeded,
    Cancelled,
    Timeout,
    Io,
    Protocol,
}

#[derive(Debug)]
pub struct AssetIoError {
    kind: AssetIoErrorKind,
    context: String,
    detail: Option<String>,
    source: Option<Box<dyn Error + Send + Sync>>,
}

impl AssetIoError {
    pub fn kind(&self) -> AssetIoErrorKind {
        self.kind
    }

    pub fn code(&self) -> &'static str {
        match self.kind {
            AssetIoErrorKind::NotFound => "asset_io.not_found",
            AssetIoErrorKind::InvalidFormat => "asset_io.invalid_format",
            AssetIoErrorKind::UnsupportedVersion => "asset_io.unsupported_version",
            AssetIoErrorKind::DigestMismatch => "asset_io.digest_mismatch",
            AssetIoErrorKind::LimitExceeded => "asset_io.limit_exceeded",
            AssetIoErrorKind::Cancelled => "asset_io.cancelled",
            AssetIoErrorKind::Timeout => "asset_io.timeout",
            AssetIoErrorKind::Io => "asset_io.io_failed",
            AssetIoErrorKind::Protocol => "asset_io.protocol_failed",
        }
    }

    pub fn invalid_format(context: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(
            AssetIoErrorKind::InvalidFormat,
            context,
            Some(detail.into()),
            None,
        )
    }

    pub fn unsupported_version(context: impl Into<String>, version: impl fmt::Display) -> Self {
        Self::new(
            AssetIoErrorKind::UnsupportedVersion,
            context,
            Some(format!("version {version}")),
            None,
        )
    }

    pub fn digest_mismatch(context: impl Into<String>) -> Self {
        Self::new(AssetIoErrorKind::DigestMismatch, context, None, None)
    }

    pub fn limit_exceeded(context: impl Into<String>, limit: impl fmt::Display) -> Self {
        Self::new(
            AssetIoErrorKind::LimitExceeded,
            context,
            Some(format!("limit {limit}")),
            None,
        )
    }

    pub fn cancelled(context: impl Into<String>) -> Self {
        Self::new(AssetIoErrorKind::Cancelled, context, None, None)
    }

    pub fn timeout(context: impl Into<String>) -> Self {
        Self::new(AssetIoErrorKind::Timeout, context, None, None)
    }

    pub fn protocol(context: impl Into<String>, detail: impl Into<String>) -> Self {
        Self::new(
            AssetIoErrorKind::Protocol,
            context,
            Some(detail.into()),
            None,
        )
    }

    pub fn io(context: impl Into<String>, source: io::Error) -> Self {
        let kind = match source.kind() {
            io::ErrorKind::NotFound => AssetIoErrorKind::NotFound,
            io::ErrorKind::TimedOut => AssetIoErrorKind::Timeout,
            io::ErrorKind::FileTooLarge => AssetIoErrorKind::LimitExceeded,
            io::ErrorKind::InvalidData => AssetIoErrorKind::InvalidFormat,
            _ => AssetIoErrorKind::Io,
        };
        Self::new(kind, context, None, Some(Box::new(source)))
    }

    pub fn caused_by<E>(kind: AssetIoErrorKind, context: impl Into<String>, source: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self::new(kind, context, None, Some(Box::new(source)))
    }

    fn new(
        kind: AssetIoErrorKind,
        context: impl Into<String>,
        detail: Option<String>,
        source: Option<Box<dyn Error + Send + Sync>>,
    ) -> Self {
        Self {
            kind,
            context: context.into(),
            detail,
            source,
        }
    }
}

impl fmt::Display for AssetIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.context)?;
        if let Some(detail) = &self.detail {
            write!(formatter, " ({detail})")?;
        }
        if let Some(source) = &self.source {
            write!(formatter, ": {source}")?;
        }
        Ok(())
    }
}

impl Error for AssetIoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}
