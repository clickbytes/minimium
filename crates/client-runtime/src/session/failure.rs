use crate::ContentResolutionTicket;
use std::{error::Error, fmt, sync::Arc};

#[derive(Clone, Debug)]
pub struct SessionFailure {
    code: &'static str,
    message: String,
    source: Option<Arc<dyn Error + Send + Sync>>,
    content_ticket: Option<ContentResolutionTicket>,
}

impl SessionFailure {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
            content_ticket: None,
        }
    }

    pub fn caused_by<E>(code: &'static str, message: impl Into<String>, source: E) -> Self
    where
        E: Error + Send + Sync + 'static,
    {
        Self {
            code,
            message: message.into(),
            source: Some(Arc::new(source)),
            content_ticket: None,
        }
    }

    pub fn from_asset_error(error: minimium_asset_io::AssetIoError) -> Self {
        let code = error.code();
        Self::caused_by(code, error.to_string(), error)
    }

    pub fn with_content_ticket(mut self, ticket: ContentResolutionTicket) -> Self {
        self.content_ticket = Some(ticket);
        self
    }

    pub const fn content_ticket(&self) -> Option<ContentResolutionTicket> {
        self.content_ticket
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for SessionFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SessionFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn Error + 'static))
    }
}

impl PartialEq for SessionFailure {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code && self.message == other.message
    }
}

impl Eq for SessionFailure {}
