use std::fmt;

/// Stable categories shared by command-line, console, and administrative adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandErrorCode {
    Unavailable,
    StaleContext,
    Conflict,
    SettingsBaseChanged,
    RevisionConflict,
    StorageFailure,
    UnknownAfterDisconnect,
    ResultLimit,
    Busy,
    InvalidEndpoint,
    NoSession,
    QueueFull,
    Timeout,
    InputTooLong,
    UnknownCommand,
    NotCallable,
    NotAssignable,
    MissingArgument,
    UnexpectedArgument,
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
            Self::Unavailable => "unavailable",
            Self::StaleContext => "stale_context",
            Self::SettingsBaseChanged => "settings_base_changed",
            Self::RevisionConflict => "revision_conflict",
            Self::StorageFailure => "storage_failure",
            Self::Conflict => "conflict",
            Self::UnknownAfterDisconnect => "unknown_after_disconnect",
            Self::ResultLimit => "result_limit",
            Self::NotCallable => "not_callable",
            Self::NotAssignable => "not_assignable",
            Self::UnexpectedArgument => "unexpected_argument",
            Self::MissingArgument => "missing_argument",
            Self::Busy => "busy",
            Self::InvalidEndpoint => "invalid_endpoint",
            Self::NoSession => "no_session",
            Self::QueueFull => "queue_full",
            Self::Timeout => "timeout",
            Self::InputTooLong => "input_too_long",
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
    pub span: Option<crate::SourceSpan>,
}

impl CommandError {
    pub fn at(mut self, span: crate::SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

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
            span: None,
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
