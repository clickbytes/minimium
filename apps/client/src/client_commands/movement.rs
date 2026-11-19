use super::{
    ArgumentSpec as Arg, ArgumentType as ArgType, ClientCommand, ClientPropertyCommand,
    ClientPropertyKey as Key, ClientPropertyValue as Value, CommandRegistry, CommandResult,
    FrontendAction, ParseError,
};

const COMMANDS: &[super::CommandSpec] = &[
    super::CommandSpec {
        name: "movement fly",
        alias: Some("fly"),
        usage: "movement fly [on|off] - noclip; Space up, Ctrl down; inherits walk speed and accel",
        arguments: &[Arg::new("enabled", ArgType::Boolean, true, false, Some(&["on", "off"]), None, None, "toggle local flight")],
        parser: parse,
    },
    super::CommandSpec {
        name: "movement speed",
        alias: None,
        usage: "movement speed [0..10000] - maximum walk/fly speed in m/s; no argument shows current value",
        arguments: &[Arg::new("speed", ArgType::Decimal, true, false, None, Some("0..=10000"), Some("m/s"), "maximum walk and flight speed")],
        parser: parse,
    },
    super::CommandSpec {
        name: "movement accel",
        alias: None,
        usage: "movement accel [0..10000] - walk/fly acceleration and braking in m/s²; no argument shows current value",
        arguments: &[Arg::new("acceleration", ArgType::Decimal, true, false, None, Some("0..=10000"), Some("m/s²"), "walk and flight acceleration and braking")],
        parser: parse,
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    let args: Vec<_> = words[1].split_whitespace().collect();
    if words[0] == "movement fly" {
        return match args.as_slice() {
            [] => Ok(ClientCommand::Property(ClientPropertyCommand::Get(
                Key::MovementFly,
            ))),
            ["on"] => Ok(ClientCommand::Property(ClientPropertyCommand::Set(
                Value::MovementFly(true),
            ))),
            ["off"] => Ok(ClientCommand::Property(ClientPropertyCommand::Set(
                Value::MovementFly(false),
            ))),
            _ => Err(ParseError::UnknownCommand),
        };
    }
    let key = if words[0] == "movement speed" {
        Key::MovementSpeed
    } else {
        Key::MovementAccel
    };
    match args.as_slice() {
        [] => Ok(ClientCommand::Property(ClientPropertyCommand::Get(key))),
        [value] => super::parse_setting_value(key.key(), value)
            .map(|value| ClientCommand::Property(ClientPropertyCommand::Set(value)))
            .map_err(ParseError::Structured),
        _ => Err(ParseError::UnknownCommand),
    }
}

pub(super) fn handle_property(command: &ClientPropertyCommand) -> Option<CommandResult> {
    use minimium_client_runtime::MovementField as Field;
    use minimium_protocol::MovementPatch as Patch;
    let defaults = minimium_simulation::MovementPolicy::default();
    let (field, patch) = match command {
        ClientPropertyCommand::Get(Key::MovementSpeed) => (Field::Speed, None),
        ClientPropertyCommand::Get(Key::MovementAccel) => (Field::Acceleration, None),
        ClientPropertyCommand::Get(Key::MovementFly) => (Field::Flight, None),
        ClientPropertyCommand::Set(Value::MovementSpeed(v)) => {
            (Field::Speed, Some(Patch::Speed(*v)))
        }
        ClientPropertyCommand::Set(Value::MovementAccel(v)) => {
            (Field::Acceleration, Some(Patch::Acceleration(*v)))
        }
        ClientPropertyCommand::Set(Value::MovementFly(v)) => {
            (Field::Flight, Some(Patch::FlightEnabled(*v)))
        }
        ClientPropertyCommand::Reset(Key::MovementSpeed) => {
            (Field::Speed, Some(Patch::Speed(defaults.speed)))
        }
        ClientPropertyCommand::Reset(Key::MovementAccel) => (
            Field::Acceleration,
            Some(Patch::Acceleration(defaults.accel)),
        ),
        ClientPropertyCommand::Reset(Key::MovementFly) => {
            (Field::Flight, Some(Patch::FlightEnabled(false)))
        }
        _ => return None,
    };
    Some(CommandResult::Action(FrontendAction::Movement(
        field, patch,
    )))
}
