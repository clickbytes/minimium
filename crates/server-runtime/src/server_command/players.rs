use super::ServerCommand;
use minimium_commands::{CommandError, CommandErrorCode};

pub(crate) fn players_page(
    cursor: Option<String>,
    limit: u16,
) -> Result<ServerCommand, CommandError> {
    if !(1..=256).contains(&limit)
        || cursor.as_ref().is_some_and(|value| {
            value.is_empty() || value.len() > 256 || value.chars().any(char::is_control)
        })
    {
        return Err(CommandError::new(
            CommandErrorCode::InvalidArguments,
            Some("server.players.list"),
            "cursor or limit is outside its bounds",
            Some("server.players.list(cursor?:Cursor, limit:U16=64)"),
        ));
    }
    Ok(ServerCommand::Players { cursor, limit })
}
