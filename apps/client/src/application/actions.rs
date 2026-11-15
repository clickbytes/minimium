use crate::session::QueueError;
use minimium_client_runtime::ConnectionController;
use minimium_protocol::{
    excavation::ExcavationInput, ClientMessage, ClientWorldInput, MovementSample,
};
use minimium_simulation::MovementIntent;
use minimium_world::entity::EntityId;
#[cfg(feature = "desktop")]
use std::time::{Duration, Instant};

#[cfg(feature = "desktop")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UseTrigger {
    Pressed,
    Held,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClientAction {
    #[cfg(feature = "desktop")]
    UseEquipped {
        direction: [f32; 3],
        trigger: UseTrigger,
    },
    FireWeapon {
        direction: [f32; 3],
    },
    Motion(MovementIntent),
    #[cfg(feature = "desktop")]
    MapTeleport {
        direction: [f32; 3],
    },
    SetAltitude {
        km: f32,
    },
    SetMovementPolicy {
        speed: f32,
        accel: f32,
    },
    SetFlightEnabled(bool),
    InteractEntity {
        entity: EntityId,
    },
    GrantTools,
    #[cfg(feature = "desktop")]
    EquipTool {
        slot: Option<u8>,
    },
    Chat(String),
    SetName(String),
    Ping,
}

#[cfg(feature = "desktop")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SuppressionReason {
    NoSession,
    SessionNotConnected,
    ToolUnavailable,
    ToolPendingOrCoolingDown,
    WeaponInputCooldown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueuedActionKind {
    WeaponFire,
    #[cfg(feature = "desktop")]
    ToolUse,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LocalActionOutcome {
    Queued {
        movement: Option<MovementSample>,
        kind: QueuedActionKind,
    },
    #[cfg(feature = "desktop")]
    Suppressed(SuppressionReason),
    Rejected {
        kind: QueuedActionKind,
        error: QueueError,
    },
}

/// The single application-owned path for client-authored world actions.
pub(crate) struct ClientActionExecutor {
    #[cfg(feature = "desktop")]
    last_input_fire_at: Option<Instant>,
}

impl ClientActionExecutor {
    #[cfg(feature = "desktop")]
    pub(crate) fn clear_input_cooldown(&mut self) {
        self.last_input_fire_at = None;
    }

    pub(crate) fn execute(
        &mut self,
        connection: &mut ConnectionController,
        action: ClientAction,
    ) -> LocalActionOutcome {
        match action {
            #[cfg(feature = "desktop")]
            ClientAction::UseEquipped { direction, trigger } => {
                let Some(session) = connection.session_mut() else {
                    return LocalActionOutcome::Suppressed(SuppressionReason::NoSession);
                };
                if session.inventory().tool().is_some() {
                    return match session.queue_tool_use(direction) {
                        Ok(true) => LocalActionOutcome::Queued {
                            movement: None,
                            kind: QueuedActionKind::ToolUse,
                        },
                        #[cfg(feature = "desktop")]
                        Ok(false) => LocalActionOutcome::Suppressed(
                            SuppressionReason::ToolPendingOrCoolingDown,
                        ),
                        Err(error) => LocalActionOutcome::Rejected {
                            kind: QueuedActionKind::ToolUse,
                            error,
                        },
                    };
                }
                if trigger == UseTrigger::Held {
                    return LocalActionOutcome::Suppressed(SuppressionReason::ToolUnavailable);
                }
                if session.state() != minimium_client_runtime::SessionState::Connected {
                    return LocalActionOutcome::Suppressed(SuppressionReason::SessionNotConnected);
                }
                let now = Instant::now();
                if self.last_input_fire_at.is_some_and(|last| {
                    now.saturating_duration_since(last) < Duration::from_millis(300)
                }) {
                    return LocalActionOutcome::Suppressed(SuppressionReason::WeaponInputCooldown);
                }
                let outcome = Self::queue_world(
                    connection,
                    ClientWorldInput::Fire { direction },
                    QueuedActionKind::WeaponFire,
                );
                if matches!(outcome, LocalActionOutcome::Queued { .. }) {
                    self.last_input_fire_at = Some(now);
                }
                outcome
            }
            ClientAction::FireWeapon { direction } => Self::queue_world(
                connection,
                ClientWorldInput::Fire { direction },
                QueuedActionKind::WeaponFire,
            ),
            ClientAction::Motion(intent) => Self::queue_world(
                connection,
                ClientWorldInput::Motion(MovementSample::new(intent)),
                QueuedActionKind::Other,
            ),
            #[cfg(feature = "desktop")]
            ClientAction::MapTeleport { direction } => Self::queue_world(
                connection,
                ClientWorldInput::MapTeleport { direction },
                QueuedActionKind::Other,
            ),
            ClientAction::SetAltitude { km } => Self::queue_world(
                connection,
                ClientWorldInput::SetAltitude { km },
                QueuedActionKind::Other,
            ),
            ClientAction::SetMovementPolicy { speed, accel } => Self::queue_world(
                connection,
                ClientWorldInput::SetMovementPolicy { speed, accel },
                QueuedActionKind::Other,
            ),
            ClientAction::SetFlightEnabled(value) => Self::queue_world(
                connection,
                ClientWorldInput::SetFlightEnabled(value),
                QueuedActionKind::Other,
            ),
            ClientAction::InteractEntity { entity } => Self::queue_world(
                connection,
                ClientWorldInput::InteractEntity { entity },
                QueuedActionKind::Other,
            ),
            ClientAction::GrantTools => Self::queue_world(
                connection,
                ClientWorldInput::Excavation(ExcavationInput::GrantTools),
                QueuedActionKind::Other,
            ),
            #[cfg(feature = "desktop")]
            ClientAction::EquipTool { slot } => Self::queue_world(
                connection,
                ClientWorldInput::Excavation(ExcavationInput::Equip { slot }),
                QueuedActionKind::Other,
            ),
            ClientAction::Chat(text) => Self::queue_world(
                connection,
                ClientWorldInput::Chat(text),
                QueuedActionKind::Other,
            ),
            ClientAction::SetName(name) => Self::queue_world(
                connection,
                ClientWorldInput::SetName(name),
                QueuedActionKind::Other,
            ),
            ClientAction::Ping => match connection.queue(&ClientMessage::Ping) {
                Ok(()) => LocalActionOutcome::Queued {
                    movement: None,
                    kind: QueuedActionKind::Other,
                },
                Err(error) => LocalActionOutcome::Rejected {
                    kind: QueuedActionKind::Other,
                    error,
                },
            },
        }
    }

    fn queue_world(
        connection: &mut ConnectionController,
        input: ClientWorldInput,
        kind: QueuedActionKind,
    ) -> LocalActionOutcome {
        match connection
            .session_mut()
            .ok_or(QueueError::Disconnected)
            .and_then(|session| session.queue_world_input(input))
        {
            Ok(movement) => LocalActionOutcome::Queued { movement, kind },
            Err(error) => LocalActionOutcome::Rejected { kind, error },
        }
    }
}

impl Default for ClientActionExecutor {
    fn default() -> Self {
        Self {
            #[cfg(feature = "desktop")]
            last_input_fire_at: None,
        }
    }
}
