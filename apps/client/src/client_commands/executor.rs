use super::*;

pub(crate) fn execute(command: ClientCommand, context: &mut CommandContext) -> CommandResult {
    match command {
        ClientCommand::Presentation(operation) => {
            return CommandResult::Action(FrontendAction::Presentation(operation));
        }
        ClientCommand::AtmosphereDebug(mode) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::AtmosphereDebug(mode),
            )))
        }
        ClientCommand::SetAltitude(km) => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Altitude(km),
            ))
        }
        ClientCommand::Goto {
            latitude,
            longitude,
        } => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Goto {
                    latitude,
                    longitude,
                },
            ))
        }
        ClientCommand::ConfigSave(path) => {
            return CommandResult::Action(FrontendAction::ConfigSave(path))
        }
        ClientCommand::Print(text) => return CommandResult::Action(FrontendAction::Print(text)),
        ClientCommand::TerrainDebug(mode) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::TerrainDebug(mode),
            )))
        }
        ClientCommand::WeatherDebug(mode) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::CloudDebug(mode),
            )))
        }
        ClientCommand::WeatherDetail(level) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::CloudDetail(level),
            )))
        }
        ClientCommand::Ping => return CommandResult::Action(FrontendAction::Ping),
        ClientCommand::Move(direction) => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Move(direction),
            ))
        }
        ClientCommand::Jump => return CommandResult::Action(FrontendAction::Jump),
        ClientCommand::InteractPortal => {
            if context.player_position.is_none() {
                return CommandResult::Rejected(CommandError::new(
                    CommandErrorCode::Unavailable,
                    Some("player.interaction.portal"),
                    "snapshot unavailable",
                    None::<String>,
                ));
            }
            let Some(entity) = context.portal_entity else {
                return CommandResult::Rejected(CommandError::new(
                    CommandErrorCode::Unavailable,
                    Some("player.interaction.portal"),
                    "portal unavailable",
                    None::<String>,
                ));
            };
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Portal(entity),
            ));
        }
        ClientCommand::MapTeleport => return CommandResult::Action(FrontendAction::MapTeleport),
        ClientCommand::Fire => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Fire(context.fire_direction),
            ))
        }
        ClientCommand::GrantTools => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::GrantTools,
            ))
        }
        ClientCommand::Chat(text) => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Chat(text),
            ))
        }
        ClientCommand::SetName(name) => {
            return CommandResult::Action(FrontendAction::Player(
                player_actions::PlayerIntent::Name(name),
            ))
        }
        ClientCommand::Help => return CommandResult::Action(FrontendAction::Help),
        ClientCommand::Connect(address) => {
            return CommandResult::Action(FrontendAction::Connect(address))
        }
        ClientCommand::HostLocal(lane) => {
            return CommandResult::Action(FrontendAction::HostLocal(lane))
        }
        ClientCommand::Disconnect => return CommandResult::Action(FrontendAction::Disconnect),
        ClientCommand::FpsShow => return CommandResult::Action(FrontendAction::FpsShow),
        ClientCommand::NetStats => return CommandResult::Action(FrontendAction::NetStats),
        ClientCommand::State => return CommandResult::Action(FrontendAction::State),
        ClientCommand::Clear => return CommandResult::Action(FrontendAction::Clear),
        ClientCommand::Quit => return CommandResult::Action(FrontendAction::Quit),
        ClientCommand::SkyQuality(quality) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::SkyQuality(quality),
            )))
        }
        ClientCommand::SkyDemo(enabled) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::SkyDemo(enabled),
            )))
        }
        ClientCommand::Fov(Some(degrees)) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::CameraFov(degrees),
            )))
        }
        ClientCommand::Fov(None) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Get(
                ClientPropertyKey::CameraFov,
            )))
        }
        ClientCommand::Property(command) => {
            if let Some(clock) = ClockAction::from_property(&command) {
                return CommandResult::Action(FrontendAction::Clock(clock));
            }
            if let Some(result) = movement::handle_property(&command) {
                return result;
            }
            return CommandResult::Action(FrontendAction::Property(command));
        }
        ClientCommand::ShowCelestialNames(enabled) => {
            return CommandResult::Action(FrontendAction::Property(ClientPropertyCommand::Set(
                ClientPropertyValue::SkyNames(enabled),
            )))
        }
        ClientCommand::SetTime { minute, local } => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::Write(
                ClockWrite::HourMinute {
                    minute,
                    solar: local,
                },
            )))
        }
        ClientCommand::SetInstant(utc) => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::Write(
                ClockWrite::Set(utc),
            )))
        }
        ClientCommand::CameraLook { yaw, pitch } => {
            return CommandResult::Action(FrontendAction::CameraLook { yaw, pitch })
        }
        ClientCommand::CameraStatus => return CommandResult::Action(FrontendAction::CameraStatus),
        ClientCommand::InitialClockConfig => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::InitialConfig))
        }
        ClientCommand::TimeStatus => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::Status))
        }
        ClientCommand::ClockRead(path) => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::Read(path)))
        }
        ClientCommand::ClockQuery(path) => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::Query(path)))
        }
        ClientCommand::TimeScale(value) => {
            return CommandResult::Action(FrontendAction::Clock(ClockAction::Scale(value)))
        }
        ClientCommand::SpawnRandomBolid(count) => {
            return CommandResult::Action(FrontendAction::SpawnRandomBolid(count))
        }
        ClientCommand::SpawnRandomMeteorid => {
            return CommandResult::Action(FrontendAction::SpawnRandomMeteorid)
        }
        ClientCommand::BolidStatus(id) => {
            return CommandResult::Action(FrontendAction::BolidStatus(id))
        }
        ClientCommand::SaveWorld => return CommandResult::Action(FrontendAction::SaveWorld),
        ClientCommand::SaveStatus(id) => {
            return CommandResult::Action(FrontendAction::SaveStatus(id))
        }
        ClientCommand::Script(control) => {
            return CommandResult::Action(FrontendAction::Script(control))
        }
        ClientCommand::ImpactStatus { world, id } => {
            return CommandResult::Action(FrontendAction::ImpactStatus { world, id })
        }
    }
}
