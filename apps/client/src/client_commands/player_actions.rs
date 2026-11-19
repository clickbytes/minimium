use super::output::{ClientPayload, ClientResult};
use super::CommandSource;
use crate::application::ClientAction;
use minimium_commands::results::{OperationContext, OperationReceipt, OperationResult, Stage};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PlayerIntent {
    Move([f32; 2]),
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
}

impl PlayerIntent {
    pub(crate) fn prepare(
        &self,
        source: CommandSource,
    ) -> (ClientAction, Option<([f32; 3], AimSource)>) {
        let action = match self {
            Self::Move(direction) => ClientAction::Motion(minimium_simulation::MovementIntent {
                direction: *direction,
                ..Default::default()
            }),
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
        format!("{label} world_input={}{aim}", self.sequence)
    }
}
