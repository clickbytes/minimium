use std::fmt;

/// Stable categories shared by command-line, console, and administrative adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandErrorCode {
    UnknownCommand,
    InvalidArguments,
    UnknownKey,
    UnknownProperty,
    InvalidValue,
    MissingValue,
    DuplicateOption,
    ConflictingOptions,
    UnknownOption,
    HelpQuery,
    PermissionDenied,
    UnsupportedContext,
    RestartRequired,
    CreationOnly,
    ReadOnly,
    Io,
}

impl CommandErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownCommand => "unknown_command",
            Self::InvalidArguments => "invalid_arguments",
            Self::UnknownKey | Self::UnknownProperty => "unknown_key",
            Self::InvalidValue => "invalid_value",
            Self::MissingValue => "missing_value",
            Self::DuplicateOption => "duplicate_option",
            Self::ConflictingOptions => "conflicting_options",
            Self::UnknownOption => "unknown_option",
            Self::HelpQuery => "help_query",
            Self::PermissionDenied => "permission_denied",
            Self::UnsupportedContext => "unsupported_context",
            Self::RestartRequired => "restart_required",
            Self::CreationOnly => "creation_only",
            Self::ReadOnly => "read_only",
            Self::Io => "io",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommandError {
    pub code: CommandErrorCode,
    pub subject: Option<String>,
    pub reason: String,
    pub usage: Option<String>,
}

impl CommandError {
    pub fn new(
        code: CommandErrorCode,
        subject: Option<impl Into<String>>,
        reason: impl Into<String>,
        usage: Option<impl Into<String>>,
    ) -> Self {
        Self {
            code,
            subject: subject.map(Into::into),
            reason: reason.into(),
            usage: usage.map(Into::into),
        }
    }
}

impl fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.reason)?;
        if let Some(usage) = &self.usage {
            write!(formatter, "\nusage: {usage}")?;
        }
        Ok(())
    }
}

impl std::error::Error for CommandError {}
