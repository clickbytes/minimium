use super::{CommandError, CommandErrorCode, ServerCommand};

pub(super) fn parse(words: &[&str]) -> Result<ServerCommand, CommandError> {
    if matches!(words, ["world.time.diagnostics", ""]) {
        Ok(ServerCommand::TimeDiagnostics)
    } else {
        Err(CommandError::new(
            CommandErrorCode::InvalidArguments,
            Some("world.time.diagnostics"),
            "method takes no arguments",
            Some("world.time.diagnostics()"),
        ))
    }
}
