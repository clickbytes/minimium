use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};

const COMMANDS: &[CommandSpec] = &[
    CommandSpec { name: "movement fly", alias: Some("fly"),
        usage: "movement fly [on|off] - noclip; Space up, Ctrl down; inherits walk speed and accel", parser: parse },
    CommandSpec { name: "movement speed", alias: None,
        usage: "movement speed [0..10000] - maximum walk/fly speed in m/s; no argument shows current value", parser: parse },
    CommandSpec { name: "movement accel", alias: None,
        usage: "movement accel [0..10000] - walk/fly acceleration and braking in m/s²; no argument shows current value", parser: parse },
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Setting {
    Fly(Option<bool>),
    Speed(Option<f32>),
    Accel(Option<f32>),
}

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    let args: Vec<_> = words[1].split_whitespace().collect();
    let setting = match words[0] {
        "movement fly" => Setting::Fly(match args.as_slice() {
            [] => None,
            ["on"] => Some(true),
            ["off"] => Some(false),
            _ => return Err(ParseError::UnknownCommand),
        }),
        command => {
            let value = match args.as_slice() {
                [] => None,
                [value] => Some(
                    value
                        .parse::<f32>()
                        .ok()
                        .filter(|v| {
                            v.is_finite() && (0.0..=minimium_simulation::MOVEMENT_LIMIT).contains(v)
                        })
                        .ok_or(ParseError::UnknownCommand)?,
                ),
                _ => return Err(ParseError::UnknownCommand),
            };
            if command == "movement speed" {
                Setting::Speed(value)
            } else {
                Setting::Accel(value)
            }
        }
    };
    Ok(ClientCommand::Movement(setting))
}

pub(super) fn execute(
    setting: Setting,
    context: &mut super::CommandContext<'_>,
) -> super::CommandResult {
    let mut settings = context.connection.movement;
    let change = match setting {
        Setting::Fly(value) => {
            if let Some(v) = value {
                settings.fly = v;
            }
            value.is_some()
        }
        Setting::Speed(value) => {
            if let Some(v) = value {
                settings.speed = v;
            }
            value.is_some()
        }
        Setting::Accel(value) => {
            if let Some(v) = value {
                settings.accel = v;
            }
            value.is_some()
        }
    };
    if change {
        let input = minimium_simulation::CharacterInput {
            movement: settings,
            ..Default::default()
        };
        if let Err(error) = context
            .connection
            .queue(&minimium_protocol::ClientMessage::Motion(input))
        {
            return super::CommandResult::QueueError(error);
        }
        context.connection.movement = settings;
    }
    super::CommandResult::Action(super::FrontendAction::Print(format!(
        "movement: fly={} (noclip), speed={} m/s, accel={} m/s² (shared by walk and fly; range 0..10000)",
        settings.fly, settings.speed, settings.accel)))
}
