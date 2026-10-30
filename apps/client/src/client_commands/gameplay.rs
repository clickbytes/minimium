use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "move",
        alias: None,
        usage: "move X Z",
        parser: parse,
    },
    CommandSpec {
        name: "jump",
        alias: None,
        usage: "jump",
        parser: parse,
    },
    CommandSpec {
        name: "portal",
        alias: None,
        usage: "portal",
        parser: parse,
    },
    CommandSpec {
        name: "map_telepo",
        alias: None,
        usage: "map_telepo: click land on the Earth overview",
        parser: parse,
    },
    CommandSpec {
        name: "fire",
        alias: None,
        usage: "fire",
        parser: parse,
    },
    CommandSpec {
        name: "say",
        alias: Some("chat"),
        usage: "say TEXT (alias: chat)",
        parser: parse,
    },
    CommandSpec {
        name: "name",
        alias: None,
        usage: "name NAME",
        parser: parse,
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    match words {
        ["move", x, z] => Ok(ClientCommand::Move([
            parse_coordinate(x)?,
            parse_coordinate(z)?,
        ])),
        ["move", ..] => Err(ParseError::MoveUsage),
        ["jump"] => Ok(ClientCommand::Jump),
        ["portal"] => Ok(ClientCommand::UsePortal),
        ["map_telepo"] => Ok(ClientCommand::MapTeleport),
        ["fire"] => Ok(ClientCommand::Fire),
        ["say", text @ ..] | ["chat", text @ ..] => {
            let message = text.join(" ");
            if message.is_empty() {
                Err(ParseError::MissingChat)
            } else {
                Ok(ClientCommand::Chat(message))
            }
        }
        ["name", name] => Ok(ClientCommand::SetName((*name).to_owned())),
        ["name", ..] => Err(ParseError::NameUsage),
        _ => Err(ParseError::UnknownCommand),
    }
}

fn parse_coordinate(value: &str) -> Result<f32, ParseError> {
    let coordinate = value
        .parse::<f32>()
        .map_err(|_| ParseError::InvalidCoordinate)?;
    if !coordinate.is_finite() {
        return Err(ParseError::NonFiniteCoordinate);
    }
    Ok(coordinate)
}
