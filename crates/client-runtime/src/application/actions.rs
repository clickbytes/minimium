use crate::{ClientTime, ClientTimeError, ConnectionController, QueueError};
use minimium_protocol::{
    excavation::ExcavationInput, ClientMessage, ClientWorldInput, MovementSample,
};
use minimium_simulation::MovementIntent;
use minimium_world::entity::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UseTrigger {
    Pressed,
    Held,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ClientAction {
    UseEquipped {
        direction: [f32; 3],
        trigger: UseTrigger,
    },
    FireWeapon {
        direction: [f32; 3],
    },
    Motion(MovementIntent),
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
    EquipTool {
        slot: Option<u8>,
    },
    Chat(String),
    SetName(String),
    Ping,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuppressionReason {
    Paused,
    NoSession,
    SessionNotConnected,
    ToolUnavailable,
    ToolPendingOrCoolingDown,
    WeaponInputCooldown,
    MapTeleportPending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueuedActionKind {
    WeaponFire,
    ToolUse,
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LocalActionOutcome {
    Queued {
        movement: Option<MovementSample>,
        kind: QueuedActionKind,
    },
    Suppressed(SuppressionReason),
    Rejected {
        kind: QueuedActionKind,
        error: QueueError,
    },
}

/// The single application-owned path for client-authored world actions.
pub(crate) struct ClientActionExecutor {
    last_operation_at: ClientTime,
    last_input_fire_at: Option<u64>,
}

impl ClientActionExecutor {
    pub(crate) fn new(now: ClientTime) -> Self {
        Self {
            last_operation_at: now,
            last_input_fire_at: None,
        }
    }

    pub(crate) fn validate_time(&self, now: ClientTime) -> Result<(), ClientTimeError> {
        now.validate_after(self.last_operation_at)
    }

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
        let sample = connection.session().and_then(|s| s.clock_sample());
        if sample.is_some_and(|s| s.rate_ppm().get() == 0)
            && !matches!(
                action,
                ClientAction::Ping | ClientAction::SetName(_) | ClientAction::Chat(_)
            )
        {
            return LocalActionOutcome::Suppressed(SuppressionReason::Paused);
        }
        let game_ms = sample.map_or(0, |s| s.boundary().simulation_ms());
        match action {
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
                if session.state() != crate::SessionState::Connected {
                    return LocalActionOutcome::Suppressed(SuppressionReason::SessionNotConnected);
                }
                if self
                    .last_input_fire_at
                    .is_some_and(|last| game_ms.saturating_sub(last) < 300)
                {
                    return LocalActionOutcome::Suppressed(SuppressionReason::WeaponInputCooldown);
                }
                let outcome = Self::queue_world(
                    connection,
                    ClientWorldInput::Fire { direction },
                    QueuedActionKind::WeaponFire,
                    now,
                );
                if matches!(outcome, LocalActionOutcome::Queued { .. }) {
                    self.last_input_fire_at = Some(game_ms);
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
