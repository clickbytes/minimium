use super::ClientApplication;
use minimium_protocol::ServerMessage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ApplicationMessageError {
    MissingWorldContent,
    WorldContentMismatch,
    StaleWorldContext,
    VisiblePlayerLimitExceeded,
    #[cfg(feature = "desktop")]
    RequiredEarthUnavailable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ApplicationTransferPreparation {
    pub(crate) ticket: minimium_client_runtime::TransferTicket,
    pub(crate) map_fingerprint: u64,
    pub(crate) manifest: minimium_world::world::WorldManifest,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorldContextChange {
    pub(crate) previous_context: Option<minimium_protocol::WorldContext>,
    pub(crate) previous_instance: minimium_world::world::WorldInstanceId,
    pub(crate) context: minimium_protocol::WorldContext,
    pub(crate) map_fingerprint: u64,
    pub(crate) position: [f32; 3],
    pub(crate) position_mm: minimium_world::world::Position3Mm,
    pub(crate) health: u16,
    pub(crate) manifest: minimium_world::world::WorldManifest,
}

impl ClientApplication {
    /// Apply session data that changes the CPU game view. Host presentation
    /// effects such as GPU installation remain separate from this state update.
    pub(crate) fn apply_game_message(
        &mut self,
        message: &ServerMessage,
    ) -> Result<(), ApplicationMessageError> {
        match message {
            ServerMessage::Welcome { .. } | ServerMessage::TransitionCommitted { .. } => {
                self.apply_world_message(message).map(|_| ())
            }
            ServerMessage::TransitionPrepare {
                transition_id,
                target_context,
                map_fingerprint,
                manifest,
                ..
            } => {
                let ticket =
                    minimium_client_runtime::TransferTicket::new(*transition_id, *target_context);
                match self.pending_transfer {
                    Some(current) if current.ticket == ticket => Ok(()),
                    _ => {
                        self.pending_transfer = Some(ApplicationTransferPreparation {
                            ticket,
                            map_fingerprint: *map_fingerprint,
                            manifest: *manifest,
                        });
                        Ok(())
                    }
                }
            }
            ServerMessage::TransitionCancelled { transition_id, .. } => {
                if self
                    .pending_transfer
                    .is_some_and(|preparation| preparation.ticket.transition_id() == *transition_id)
                {
                    self.pending_transfer = None;
                }
                Ok(())
            }
            ServerMessage::Snapshot {
                context,
                server_time_ms,
                server_tick,
                full_snapshot,
                resolved_input_seq,
                self_position,
                self_position_mm,
                self_velocity,
                self_grounded,
                self_movement,
                players,
                removed_players,
            } => {
                if self.game.session_context() != Some(*context) {
                    return Err(ApplicationMessageError::StaleWorldContext);
                }
                let geography = self
                    .connection
                    .session()
                    .and_then(|session| session.prepared_geography());
                let manifest = self
                    .game
                    .world_manifests
                    .get(&self.game.current_instance)
                    .copied();
                if !self.game.gameplay.apply_snapshot(
                    *server_time_ms,
                    *full_snapshot,
                    *server_tick,
                    *resolved_input_seq,
                    *self_position,
                    *self_position_mm,
                    *self_velocity,
                    *self_grounded,
                    *self_movement,
                    self.connection.movement,
                    manifest,
                    geography,
                    players,
                    removed_players,
                ) {
                    return Err(ApplicationMessageError::VisiblePlayerLimitExceeded);
                }
                Ok(())
            }
            ServerMessage::AltitudeResult {
                context,
                position_mm,
            } => self
                .apply_altitude_result(*context, *position_mm)
                .then_some(())
                .ok_or(ApplicationMessageError::StaleWorldContext),
            ServerMessage::HealthUpdate(health) => self
                .apply_health_update(*health)
                .then_some(())
                .ok_or(ApplicationMessageError::StaleWorldContext),
            #[cfg(feature = "desktop")]
            ServerMessage::MapTeleportResult { position_mm } => self
                .apply_map_teleport_position(*position_mm)
                .then_some(())
                .ok_or(ApplicationMessageError::StaleWorldContext),
            _ => Ok(()),
        }
    }

    pub(crate) fn apply_world_message(
        &mut self,
        message: &ServerMessage,
    ) -> Result<WorldContextChange, ApplicationMessageError> {
        let change = match message {
            ServerMessage::Welcome {
                context,
                map_fingerprint,
                position,
                position_mm,
                health,
                manifest,
                ..
            } => self.apply_world_context(
                *context,
                *map_fingerprint,
                *position,
                *position_mm,
                *health,
                *manifest,
            )?,
            ServerMessage::TransitionCommitted {
                transition_id,
                context,
                map_fingerprint,
                destination,
                destination_mm,
                health,
                manifest,
                ..
            } => {
                if !self.pending_transfer.is_some_and(|preparation| {
                    preparation.ticket.transition_id() == *transition_id
                        && preparation.ticket.context() == *context
                }) {
                    return Err(ApplicationMessageError::StaleWorldContext);
                }
                let change = self.apply_world_context(
                    *context,
                    *map_fingerprint,
                    *destination,
                    *destination_mm,
                    *health,
                    *manifest,
                )?;
                if self.pending_transfer.is_some_and(|preparation| {
                    preparation.ticket.transition_id() == *transition_id
                        && preparation.ticket.context() == *context
                }) {
                    self.pending_transfer = None;
                }
                change
            }
            _ => return Err(ApplicationMessageError::StaleWorldContext),
        };
        Ok(change)
    }

    fn apply_world_context(
        &mut self,
        context: minimium_protocol::WorldContext,
        map_fingerprint: u64,
        position: [f32; 3],
        position_mm: minimium_world::world::Position3Mm,
        health: u16,
        manifest: minimium_world::world::WorldManifest,
    ) -> Result<WorldContextChange, ApplicationMessageError> {
        let content = self
            .connection
            .session()
            .and_then(|session| session.resolved_world_content_for_context(context))
            .ok_or(ApplicationMessageError::MissingWorldContent)?;
        if content.manifest() != manifest || content.map().fingerprint() != map_fingerprint {
            return Err(ApplicationMessageError::WorldContentMismatch);
        }
        let previous_context = self.game.session_context();
        let previous_instance = self.game.current_instance;
        self.world_catalog
            .insert(context.instance(), (**content.map()).clone());
        self.game
            .world_manifests
            .insert(context.instance(), manifest);
        self.game.current_instance = context.instance();
        self.game.set_session_context(Some(context));
        self.game.gameplay.health = health;
        #[cfg(feature = "desktop")]
        self.action_executor.clear_input_cooldown();
        self.game.gameplay.clear_remote_players();
        self.game.gameplay.prediction.reset();
        let gravity =
            minimium_world::world::GravityPolicy::for_manifest(manifest).character_gravity();
        self.game.gameplay.prediction.character =
            minimium_simulation::CharacterState::at_spawn_with_gravity(position, gravity);
        self.game.gameplay.prediction.position_mm = manifest.planet().map(|_| position_mm);
        self.game.gameplay.collision_index = minimium_simulation::CollisionIndex::default();
        self.game.gameplay.collision_revision =
            self.game.gameplay.collision_revision.wrapping_add(1).max(1);
        self.game.gameplay.sectors_ready = false;
        Ok(WorldContextChange {
            previous_context,
            previous_instance,
            context,
            map_fingerprint,
            position,
            position_mm,
            health,
            manifest,
        })
    }
}
