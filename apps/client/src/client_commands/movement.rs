use super::{
    ArgumentSpec as Arg, ArgumentType as ArgType, ClientCommand, ClientPropertyCommand,
    ClientPropertyKey as Key, ClientPropertyValue as Value, CommandContext, CommandRegistry,
    CommandResult, FrontendAction, ParseError,
};

const COMMANDS: &[super::CommandSpec] = &[
    super::CommandSpec {
        name: "set_altitude",
        alias: None,
        usage: "set_altitude <0..10000 km> - height above local ground; requires fly on",
        arguments: &[Arg::new("altitude", ArgType::Decimal, false, false, None, Some("0..=10000"), Some("km"), "height above local ground")],
        parser: parse,
    },
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
    if words[0] == "set_altitude" {
        return match args.as_slice() {
            [value] => value
                .parse::<f32>()
                .ok()
                .filter(|v| v.is_finite() && (0.0..=10000.0).contains(v))
                .map(ClientCommand::SetAltitude)
                .ok_or(ParseError::UnknownCommand),
            _ => Err(ParseError::UnknownCommand),
        };
    }
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

pub(super) fn handle_property(
    command: ClientPropertyCommand,
    context: &mut CommandContext<'_>,
) -> Option<CommandResult> {
    let setting = match command {
        ClientPropertyCommand::Get(Key::MovementFly) => {
            return Some(report(Key::MovementFly, context))
        }
        ClientPropertyCommand::Get(Key::MovementSpeed) => {
            return Some(report(Key::MovementSpeed, context))
        }
        ClientPropertyCommand::Get(Key::MovementAccel) => {
            return Some(report(Key::MovementAccel, context))
        }
        ClientPropertyCommand::Set(Value::MovementFly(value)) => Setting::Fly(Some(value)),
        ClientPropertyCommand::Set(Value::MovementSpeed(value)) => Setting::Speed(Some(value)),
        ClientPropertyCommand::Set(Value::MovementAccel(value)) => Setting::Accel(Some(value)),
        ClientPropertyCommand::Reset(Key::MovementFly) => Setting::Fly(Some(false)),
        ClientPropertyCommand::Reset(Key::MovementSpeed) => Setting::Speed(Some(5.0)),
        ClientPropertyCommand::Reset(Key::MovementAccel) => Setting::Accel(Some(30.0)),
        _ => return None,
    };
    Some(apply(setting, context))
}

#[derive(Clone, Copy)]
enum Setting {
    Fly(Option<bool>),
    Speed(Option<f32>),
    Accel(Option<f32>),
}

fn apply(setting: Setting, context: &mut CommandContext<'_>) -> CommandResult {
    let mut settings = context.connection.movement;
    let change = match setting {
        Setting::Fly(value) => {
            if let Some(value) = value {
                if let Err(error) = context
                    .connection
                    .queue_world_input(minimium_protocol::ClientWorldInput::SetFlightEnabled(value))
                {
                    return CommandResult::QueueError(error);
                }
                return CommandResult::Action(FrontendAction::Print(format_movement(
                    settings,
                    context.connection.flight_enabled,
                    true,
                )));
            }
            false
        }
        Setting::Speed(value) => {
            if let Some(value) = value {
                settings.speed = value;
            }
            value.is_some()
        }
        Setting::Accel(value) => {
            if let Some(value) = value {
                settings.accel = value;
            }
            value.is_some()
        }
    };
    let queued = if change {
        if let Err(error) = context.connection.queue_world_input(
            minimium_protocol::ClientWorldInput::SetMovementPolicy {
                speed: settings.speed,
                accel: settings.accel,
            },
        ) {
            return CommandResult::QueueError(error);
        }
        true
    } else {
        false
    };
    CommandResult::Action(FrontendAction::Print(format_movement(
        settings,
        context.connection.flight_enabled,
        queued,
    )))
}

fn report(key: Key, context: &CommandContext<'_>) -> CommandResult {
    let settings = context.connection.movement;
    let value = match key {
        Key::MovementFly => context.connection.flight_enabled.to_string(),
        Key::MovementSpeed => settings.speed.to_string(),
        Key::MovementAccel => settings.accel.to_string(),
        _ => unreachable!("movement property key was matched"),
    };
    CommandResult::Action(FrontendAction::Print(format!(
        "{} requested={} effective={} source=server_policy revision={}",
        key.key(),
        value,
        value,
        settings.revision
    )))
}

fn format_movement(
    settings: minimium_simulation::MovementPolicy,
    flight_enabled: bool,
    queued: bool,
) -> String {
    format!(
        "movement: fly={} (noclip), speed={} m/s, accel={} m/s² source=server_policy revision={} request={}",
        flight_enabled && settings.can_fly,
        settings.speed,
        settings.accel,
        settings.revision,
        if queued { "accepted" } else { "unchanged" }
    )
}
