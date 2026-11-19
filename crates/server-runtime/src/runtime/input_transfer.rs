use super::*;
use crate::connection::queue_message;
impl ServerRuntime {
    pub(super) fn begin_entity_transfer(
        &mut self,
        id: minimium_core::ids::PlayerId,
        source_context: minimium_protocol::WorldContext,
        target_world: minimium_world::world::WorldInstanceId,
        destination: [f32; 3],
    ) -> crate::modules::TransferOutcome {
        use crate::modules::{TransferOutcome, TransferRejection};

        let return_location = self.operations.player_store.return_location_for(id);
        let terrain =
            super::gameplay_terrain::build(&self.game.worlds, self.prepared_geography.as_ref());
        let reads: std::collections::HashMap<_, &dyn minimium_world::terrain::TerrainRead> =
            terrain
                .iter()
                .map(|(instance, read)| {
                    (*instance, read as &dyn minimium_world::terrain::TerrainRead)
                })
                .collect();
        let offer = self.game.worlds.prepare_entity_transfer_with_terrain(
            id,
            source_context,
            target_world,
            destination,
            return_location,
            &reads,
            self.now,
        );
        let offer = match offer {
            Ok(offer) => offer,
            Err(crate::world_registry::TransferRequestRejection::AlreadyPending) => {
                return TransferOutcome::Rejected(TransferRejection::AlreadyPending)
            }
            Err(crate::world_registry::TransferRequestRejection::Validation) => {
                return TransferOutcome::Rejected(TransferRejection::Validation)
            }
            Err(crate::world_registry::TransferRequestRejection::IdentifierSpaceExhausted) => {
                return TransferOutcome::Rejected(TransferRejection::IdentifierSpaceExhausted)
            }
        };
        let Some(index) = self.connections.client_indices_by_player.get(&id).copied() else {
            self.game.worlds.cancel_transfer(id, offer.transition_id);
            return TransferOutcome::Rejected(TransferRejection::ClientUnavailable);
        };
        let (clock_sample, environment) =
            match self.environment_state_for_instance(offer.target_context.instance(), self.now) {
                Ok(state) => state,
                Err(error) => {
                    crate::diagnostics::emit(
                        &self.server_logger,
                        minimium_diagnostics::Level::Error,
                        "server.transfer_environment_failed",
                        &[
                            ("player_id", id.to_string()),
                            ("error_code", error.code().to_owned()),
                            ("reason", error.to_string()),
                        ],
                    );
                    self.game.worlds.cancel_transfer(id, offer.transition_id);
                    return TransferOutcome::Rejected(TransferRejection::EnvironmentUnavailable);
                }
            };
        let Some(client) = self.connections.clients.get_mut(index) else {
            self.game.worlds.cancel_transfer(id, offer.transition_id);
            return TransferOutcome::Rejected(TransferRejection::ClientUnavailable);
        };
        if queue_message(
            client,
            &mut self.metrics,
            minimium_protocol::ServerMessage::TransitionPrepare {
                transition_id: offer.transition_id,
                source_context: offer.source_context,
                target_context: offer.target_context,
                map_fingerprint: offer.map_fingerprint,
                destination: offer.destination,
                destination_mm: offer.destination_mm,
                manifest: offer.manifest,
                clock_sample,
                environment,
            },
        ) {
            TransferOutcome::Started
        } else {
            self.game.worlds.cancel_transfer(id, offer.transition_id);
            self.metrics.dropped_messages = self.metrics.dropped_messages.saturating_add(1);
            TransferOutcome::Rejected(TransferRejection::Backpressure)
        }
    }

    pub(super) fn map_teleport(
        &mut self,
        id: minimium_core::ids::PlayerId,
        context: minimium_protocol::WorldContext,
        direction: [f32; 3],
    ) -> Option<minimium_world::world::Position3Mm> {
        if !self.game.worlds.can_map_teleport(id, context) {
            return None;
        }
        let instance = self.game.worlds.instance_for_player(id)?;
        let manifest = self.game.worlds.manifest_for_player(id)?;
        if manifest.planet()?.mode != 1 {
            return None;
        }
        let geography = self
            .game
            .worlds
            .prepared_terrain_inputs(instance, self.prepared_geography.as_ref())
            .ok()?;
        let point = minimium_procgen::planet_point_at_direction_with_geography(
            manifest,
            direction,
            geography.clone(),
        )?;
        if point.ocean {
            return None;
        }
        let ground = minimium_procgen::planet_safe_spawn_position_at_direction_with_geography(
            manifest, direction, geography,
        )?;
        let length = ground
            .iter()
            .map(|value| (*value as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        let position_mm = minimium_world::world::Position3Mm(std::array::from_fn(|axis| {
            ground[axis] + (ground[axis] as f64 / length * 2_000.0).round() as i64
        }));
        let position = position_mm.0.map(|value| value as f32 / 1000.0);
        let world = &mut self.game.worlds.get_mut(instance)?.world;
        if !world.set_position(id, position) || !world.restore_position_mm(id, position_mm) {
            return None;
        }
        if let Some(player) = world.player(id) {
            if let Err(error) =
                self.operations
                    .player_store
                    .update_from_player(player, instance, world.gravity())
            {
                crate::diagnostics::emit(
                    &self.server_logger,
                    minimium_diagnostics::Level::Error,
                    "server.teleport_save_failed",
                    &[("player_id", id.to_string()), ("reason", error.to_string())],
                );
            }
        }
        self.game
            .active_inputs
            .insert(id, minimium_simulation::MovementIntent::default());
        self.game.input_last_received.insert(id, self.now);
        self.sky_views.remove(&id);
        self.scheduler.next_snapshot = self.now;
        Some(position_mm)
    }
}
