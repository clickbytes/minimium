use crate::session::QueueError;
use minimium_client_runtime::{ClientTime, ClientTimeError, ConnectionController};
use minimium_protocol::{
    excavation::ExcavationInput, ClientMessage, ClientWorldInput, MovementSample,
};
use minimium_simulation::MovementIntent;
use minimium_world::entity::EntityId;
#[cfg(feature = "desktop")]
use std::time::Duration;

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
    last_operation_at: ClientTime,
    #[cfg(feature = "desktop")]
    last_input_fire_at: Option<ClientTime>,
}

impl ClientActionExecutor {
    pub(crate) fn new(now: ClientTime) -> Self {
        Self {
            last_operation_at: now,
            #[cfg(feature = "desktop")]
            last_input_fire_at: None,
        }
    }

    pub(crate) fn validate_time(&self, now: ClientTime) -> Result<(), ClientTimeError> {
        now.validate_after(self.last_operation_at)
    }

    #[cfg(feature = "desktop")]
    pub(crate) fn clear_input_cooldown(&mut self) {
        self.last_input_fire_at = None;
    }

    pub(crate) fn execute(
        &mut self,
        connection: &mut ConnectionController,
        action: ClientAction,
        now: ClientTime,
    ) -> LocalActionOutcome {
        if let Err(error) = self
            .validate_time(now)
            .and_then(|()| connection.validate_time(now))
        {
            return LocalActionOutcome::Rejected {
                kind: QueuedActionKind::Other,
                error: QueueError::Time(error),
            };
        }
        self.last_operation_at = now;
        match action {
            #[cfg(feature = "desktop")]
            ClientAction::UseEquipped { direction, trigger } => {
                let Some(session) = connection.session_mut() else {
                    return LocalActionOutcome::Suppressed(SuppressionReason::NoSession);
                };
                if session.inventory().tool().is_some() {
                    return match session.queue_tool_use(direction, now) {
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
                if self.last_input_fire_at.is_some_and(|last| {
                    now.checked_duration_since(last)
                        .is_ok_and(|elapsed| elapsed < Duration::from_millis(300))
                }) {
                    return LocalActionOutcome::Suppressed(SuppressionReason::WeaponInputCooldown);
                }
                let outcome = Self::queue_world(
                    connection,
                    ClientWorldInput::Fire { direction },
                    QueuedActionKind::WeaponFire,
                    now,
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
                now,
            ),
            ClientAction::Motion(intent) => Self::queue_world(
                connection,
                ClientWorldInput::Motion(MovementSample::new(intent)),
                QueuedActionKind::Other,
                now,
            ),
            #[cfg(feature = "desktop")]
            ClientAction::MapTeleport { direction } => Self::queue_world(
                connection,
                ClientWorldInput::MapTeleport { direction },
                QueuedActionKind::Other,
                now,
            ),
            ClientAction::SetAltitude { km } => Self::queue_world(
                connection,
                ClientWorldInput::SetAltitude { km },
                QueuedActionKind::Other,
                now,
            ),
            ClientAction::SetMovementPolicy { speed, accel } => Self::queue_world(
                connection,
                ClientWorldInput::SetMovementPolicy { speed, accel },
                QueuedActionKind::Other,
                now,
            ),
            ClientAction::SetFlightEnabled(value) => Self::queue_world(
                connection,
                ClientWorldInput::SetFlightEnabled(value),
                QueuedActionKind::Other,
                now,
            ),
            ClientAction::InteractEntity { entity } => Self::queue_world(
                connection,
                ClientWorldInput::InteractEntity { entity },
                QueuedActionKind::Other,
                now,
            ),
            ClientAction::GrantTools => Self::queue_world(
                connection,
                ClientWorldInput::Excavation(ExcavationInput::GrantTools),
                QueuedActionKind::Other,
                now,
            ),
            #[cfg(feature = "desktop")]
            ClientAction::EquipTool { slot } => Self::queue_world(
                connection,
                ClientWorldInput::Excavation(ExcavationInput::Equip { slot }),
                QueuedActionKind::Other,
                now,
            ),
            ClientAction::Chat(text) => Self::queue_world(
                connection,
                ClientWorldInput::Chat(text),
                QueuedActionKind::Other,
                now,
            ),
            ClientAction::SetName(name) => Self::queue_world(
                connection,
                ClientWorldInput::SetName(name),
                QueuedActionKind::Other,
                now,
            ),
            ClientAction::Ping => match connection.queue(&ClientMessage::Ping, now) {
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
        now: ClientTime,
    ) -> LocalActionOutcome {
        match connection
            .session_mut()
            .ok_or(QueueError::Disconnected)
            .and_then(|session| session.queue_world_input(input, now))
        {
            Ok(movement) => LocalActionOutcome::Queued { movement, kind },
            Err(error) => LocalActionOutcome::Rejected { kind, error },
        }
    }
}
