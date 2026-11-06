use super::{
    ClientCommand, ClientPropertyCommand, ClientPropertyKey as Key, ClientPropertyValue as Value,
    CommandContext, CommandRegistry, CommandResult, FrontendAction, ParseError,
};

const COMMANDS: &[super::CommandSpec] = &[
    super::CommandSpec {
        name: "set_altitude",
        alias: None,
        usage: "set_altitude <0..10000 km> - height above local ground; requires fly on",
        parser: parse,
    },
    super::CommandSpec {
        name: "movement fly",
        alias: Some("fly"),
        usage: "movement fly [on|off] - noclip; Space up, Ctrl down; inherits walk speed and accel",
        parser: parse,
    },
    super::CommandSpec {
        name: "movement speed",
        alias: None,
        usage: "movement speed [0..10000] - maximum walk/fly speed in m/s; no argument shows current value",
        parser: parse,
    },
    super::CommandSpec {
        name: "movement accel",
        alias: None,
        usage: "movement accel [0..10000] - walk/fly acceleration and braking in m/s²; no argument shows current value",
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
                settings.fly = value;
            }
            value.is_some()
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
        let input = minimium_simulation::CharacterInput {
            movement: settings,
            ..Default::default()
        };
        if let Err(error) = context
            .connection
            .queue(&minimium_protocol::ClientMessage::Motion(input))
        {
            return CommandResult::QueueError(error);
        }
        context.connection.movement = settings;
        true
    } else {
        false
    };
    CommandResult::Action(FrontendAction::Print(format_movement(settings, queued)))
}

fn report(key: Key, context: &CommandContext<'_>) -> CommandResult {
    let settings = context.connection.movement;
    let value = match key {
        Key::MovementFly => settings.fly.to_string(),
        Key::MovementSpeed => settings.speed.to_string(),
        Key::MovementAccel => settings.accel.to_string(),
        _ => unreachable!("movement property key was matched"),
    };
    CommandResult::Action(FrontendAction::Print(format!(
        "{} requested={} effective={} source=local_input queue=not_submitted server_confirmation=unavailable",
        key.key(), value, value
    )))
}

fn format_movement(settings: minimium_simulation::MovementSettings, queued: bool) -> String {
    format!(
        "movement: fly={} (noclip), speed={} m/s, accel={} m/s² source=local_input queue={} server_confirmation=unavailable (shared by walk and fly; range 0..10000)",
        settings.fly,
        settings.speed,
        settings.accel,
        if queued { "accepted" } else { "unchanged" }
    )
}
