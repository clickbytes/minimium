use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerProcessErrorKind {
    Configuration,
    Startup,
    RuntimeSetup,
    StartupStatus,
    Runtime,
}

#[derive(Debug)]
pub struct ServerProcessError {
    kind: ServerProcessErrorKind,
    source: Box<dyn Error>,
}

impl ServerProcessError {
    pub fn caused_by<E>(kind: ServerProcessErrorKind, source: E) -> Self
    where
        E: Error + 'static,
    {
        Self {
            kind,
            source: Box::new(source),
        }
    }

    pub fn from_boxed(kind: ServerProcessErrorKind, source: Box<dyn Error>) -> Self {
        Self { kind, source }
    }

    pub const fn kind(&self) -> ServerProcessErrorKind {
        self.kind
    }

    pub const fn code(&self) -> &'static str {
        match self.kind {
            ServerProcessErrorKind::Configuration => "server.configuration_failed",
            ServerProcessErrorKind::Startup => "server.startup_failed",
            ServerProcessErrorKind::RuntimeSetup => "server.runtime_setup_failed",
            ServerProcessErrorKind::StartupStatus => "server.startup_status_failed",
            ServerProcessErrorKind::Runtime => "server.runtime_failed",
        }
    }
}

impl fmt::Display for ServerProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code(), self.source)
    }
}

impl Error for ServerProcessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}
