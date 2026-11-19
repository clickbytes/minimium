use super::{
    execute, parse_command, ClientCommand, CommandContext, CommandResult, FrontendAction,
    ParseError, PresentationResult,
};
use crate::application::QueuedActionKind;
use crate::session::QueueError;
use minimium_commands::{CommandError, CommandErrorCode};

pub(super) fn legacy(input: &str) -> Result<Option<ClientCommand>, CommandError> {
    if input.split_whitespace().next() == Some("dev") {
        return Err(super::presentation::unknown_dev(input));
    }
    parse_command(input).map_err(|error| match error {
        ParseError::Structured(error) => error,
        ParseError::UnknownCommand => CommandError::new(
            CommandErrorCode::UnknownCommand,
            input.split_whitespace().next(),
            "unknown command",
            Some("help"),
        ),
        ParseError::FovUsage => CommandError::new(
            CommandErrorCode::InvalidValue,
            Some("client.camera.fov"),
            "field of view must be finite and between 30 and 110 degrees",
            Some("camera fov [30..=110]"),
        ),
        ParseError::MeteorUsage => super::meteors::command_error(),
        ParseError::Usage(usage) => CommandError::new(
            CommandErrorCode::InvalidArguments,
            input.split_whitespace().next(),
            "invalid command arguments",
            Some(usage),
        ),
        other => CommandError::new(
            CommandErrorCode::InvalidArguments,
            input.split_whitespace().next(),
            format!("{other:?}"),
            None::<String>,
        ),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandSource {
    #[cfg(feature = "desktop")]
    Console,
    Terminal,
    #[cfg(feature = "desktop")]
    DeveloperStdin,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TextCommandResult {
    Empty,
    Rejected(CommandError),
    QueueRejected {
        kind: QueuedActionKind,
        error: QueueError,
    },
    Action(FrontendAction),
}

pub(crate) fn execute_text(
    input: &str,
    _source: CommandSource,
    context: &mut CommandContext,
    identity: &mut minimium_commands::results::OperationContext,
) -> TextCommandResult {
    if input.len() > minimium_commands::MAX_INPUT_BYTES {
        return TextCommandResult::Rejected(minimium_commands::input_too_long(input.len()));
    }
    let input = input.trim();
    if input.is_empty() {
        return TextCommandResult::Empty;
    }
    let command = match super::bindings::parse(input, identity) {
        Ok(Some(command)) => command,
        Ok(None) => return TextCommandResult::Empty,
        Err(error) => return TextCommandResult::Rejected(error),
    };
    match execute(command, context) {
        CommandResult::Action(action) => TextCommandResult::Action(action),
        CommandResult::Rejected(error) => TextCommandResult::Rejected(error),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommandOutput {
    Line(String),
    #[cfg(feature = "desktop")]
    UiMessage(minimium_localization::UiMessage),
    #[cfg(feature = "desktop")]
    UiError(minimium_localization::UiMessage),
    ClientText {
        message: crate::messages::ClientText,
        error: bool,
    },
    Presentation(PresentationResult),
    Clear,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct CommandActionResult {
    pub outputs: Vec<CommandOutput>,
}
