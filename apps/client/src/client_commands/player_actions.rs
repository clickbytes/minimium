use super::output::{ClientPayload, ClientResult};
use super::CommandSource;
use crate::application::ClientAction;
use minimium_commands::results::{OperationContext, OperationReceipt, OperationResult, Stage};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PlayerIntent {
    Move([f32; 2]),
    Portal(minimium_world::entity::EntityId),
    Goto {
        latitude: f32,
        longitude: f32,
    },
    #[cfg(feature = "desktop")]
    MapSelection([f32; 3]),
    Altitude(f32),
    Chat(String),
    Name(String),
    Fire([f32; 3]),
    GrantTools,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum AimSource {
    TerminalCompatibilityDefault,
    #[cfg(feature = "desktop")]
    Camera,
}

#[derive(Clone, Debug)]
pub(crate) struct PlayerReceipt {
    pub sequence: u64,
    pub aim: Option<([f32; 3], AimSource)>,
    pub navigation: Option<String>,
}

impl PlayerIntent {
    pub(crate) fn navigation_detail(&self) -> Option<String> {
        match self {
            Self::Portal(entity) => Some(format!("portal_entity={entity:?}")),
            Self::Goto {
                latitude,
                longitude,
            } => Some(format!("latitude_deg={latitude} longitude_deg={longitude}")),
            Self::Altitude(km) => Some(format!("altitude_km={km}")),
            _ => None,
        }
    }

    pub(crate) fn prepare(
        &self,
        source: CommandSource,
    ) -> (ClientAction, Option<([f32; 3], AimSource)>) {
        let action = match self {
            Self::Move(direction) => ClientAction::Motion(minimium_simulation::MovementIntent {
                direction: *direction,
                ..Default::default()
            }),
            Self::Portal(entity) => ClientAction::InteractEntity { entity: *entity },
            Self::Goto {
                latitude,
                longitude,
            } => ClientAction::MapTeleport {
                direction: super::navigation::direction(*latitude, *longitude),
            },
            #[cfg(feature = "desktop")]
            Self::MapSelection(direction) => ClientAction::MapTeleport {
                direction: *direction,
            },
            Self::Altitude(km) => ClientAction::SetAltitude { km: *km },
            Self::Chat(text) => ClientAction::Chat(text.clone()),
            Self::Name(name) => ClientAction::SetName(name.clone()),
            Self::GrantTools => ClientAction::GrantTools,
            Self::Fire(direction) => {
                let aim = match source {
                    CommandSource::Terminal => AimSource::TerminalCompatibilityDefault,
                    #[cfg(feature = "desktop")]
                    CommandSource::Console | CommandSource::DeveloperStdin => AimSource::Camera,
                };
                return (
                    ClientAction::FireWeapon {
                        direction: *direction,
                    },
                    Some((*direction, aim)),
                );
            }
        };
        (action, None)
    }
}
impl PlayerReceipt {
    pub(crate) fn result(&self, identity: OperationContext, stage: Stage) -> ClientResult {
        OperationResult::new(
            identity,
            OperationReceipt::at(stage),
            Ok(ClientPayload::Player(self.clone())),
        )
    }
    pub(crate) fn details(
        &self,
        localizer: &minimium_localization::Localizer,
        locale: minimium_localization::Locale,
    ) -> String {
        let label = localizer.format(
            locale,
            &minimium_localization::UiMessage::Notice(
                minimium_localization::UiNotice::PlayerActionReceipt,
            ),
        );
        let aim = self.aim.map_or_else(String::new, |(dir, src)| {
            format!(" direction={dir:?} aim_source={src:?}")
        });
        let navigation = self
            .navigation
            .as_deref()
            .map_or_else(String::new, |detail| format!(" {detail}"));
        format!("{label} world_input={}{aim}{navigation}", self.sequence)
    }
}
