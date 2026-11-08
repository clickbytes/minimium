use super::{
    ArgumentSpec as Arg, ArgumentType as ArgType, ClientCommand, CommandRegistry, CommandSpec,
    ParseError,
};

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "movement move",
        alias: Some("move"),
        usage: "movement move X Z (-1..1 each; tangent direction)",
        arguments: &[
            Arg::new(
                "x",
                ArgType::Decimal,
                false,
                false,
                None,
                Some("-1..=1"),
                None,
                "tangent direction x",
            ),
            Arg::new(
                "z",
                ArgType::Decimal,
                false,
                false,
                None,
                Some("-1..=1"),
                None,
                "tangent direction z",
            ),
        ],
        parser: parse,
    },
    CommandSpec {
        name: "movement jump",
        alias: Some("jump"),
        usage: "movement jump",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "world portal",
        alias: Some("portal"),
        usage: "world portal",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "world teleport",
        alias: Some("map_telepo"),
        usage: "world teleport: click land on the Earth overview",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "combat fire",
        alias: Some("fire"),
        usage: "combat fire",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "social say",
        alias: Some("chat"),
        usage: "social say TEXT (alias: chat)",
        arguments: &[Arg::new(
            "text",
            ArgType::Text,
            false,
            true,
            None,
            None,
            None,
            "message text",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "social name",
        alias: Some("name"),
        usage: "social name NAME",
        arguments: &[Arg::new(
            "name",
            ArgType::Text,
            false,
            true,
            None,
            Some("maximum 24 bytes"),
            None,
            "player display name",
        )],
        parser: parse,
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    let [command, raw_arguments] = words else {
        return Err(ParseError::UnknownCommand);
    };
    if *command == "social say" {
        return if raw_arguments.is_empty() {
            Err(ParseError::MissingChat)
        } else {
            Ok(ClientCommand::Chat((*raw_arguments).to_owned()))
        };
    }
    let arguments = raw_arguments.split_whitespace().collect::<Vec<_>>();
    match (*command, arguments.as_slice()) {
        ("movement move", [x, z]) => Ok(ClientCommand::Move([
            parse_coordinate(x)?,
            parse_coordinate(z)?,
        ])),
        ("movement move", _) => Err(ParseError::MoveUsage),
        ("movement jump", []) => Ok(ClientCommand::Jump),
        ("world portal", []) => Ok(ClientCommand::InteractPortal),
        ("world teleport", []) => Ok(ClientCommand::MapTeleport),
        ("combat fire", []) => Ok(ClientCommand::Fire),
        ("social name", [name]) => Ok(ClientCommand::SetName((*name).to_owned())),
        ("social name", _) => Err(ParseError::NameUsage),
        _ => Err(ParseError::UnknownCommand),
    }
}

fn parse_coordinate(value: &str) -> Result<f32, ParseError> {
    let coordinate = value
        .parse::<f32>()
        .map_err(|_| ParseError::InvalidCoordinate)?;
    if !coordinate.is_finite() || !(-1.0..=1.0).contains(&coordinate) {
        return Err(ParseError::NonFiniteCoordinate);
    }
    Ok(coordinate)
}
