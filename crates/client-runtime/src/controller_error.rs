use std::{error::Error, fmt, io};

#[derive(Debug)]
pub struct ControllerError {
    code: &'static str,
    message: String,
    source: Option<io::Error>,
}

impl ControllerError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    pub fn caused_by(code: &'static str, message: impl Into<String>, source: io::Error) -> Self {
        Self {
            code,
            message: message.into(),
            source: Some(source),
        }
    }

    pub fn code(&self) -> &'static str {
        self.code
    }
}

impl fmt::Display for ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl Error for ControllerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|error| error as &(dyn Error + 'static))
    }
}
