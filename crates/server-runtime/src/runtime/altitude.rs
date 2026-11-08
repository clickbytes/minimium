use super::*;
use crate::connection::queue_message;
impl ServerRuntime {
    pub(super) fn apply_altitude(
        &mut self,
        id: PlayerId,
        context: minimium_protocol::WorldContext,
        km: f32,
    ) {
        let position_mm = self.altitude_destination(id, context, km);
        if let Some(index) = self.connections.client_indices_by_player.get(&id).copied() {
            if let Some(client) = self.connections.clients.get_mut(index) {
                if !queue_message(
                    client,
                    &mut self.metrics,
                    minimium_protocol::ServerMessage::AltitudeResult {
                        context,
                        position_mm,
                    },
                ) {
                    client.disconnect_reason = Some(crate::connection::CloseReason::Backpressure);
                    self.connections.disconnected.push(index);
                }
            }
        }
    }
    fn altitude_destination(
        &mut self,
        id: PlayerId,
        context: minimium_protocol::WorldContext,
        km: f32,
    ) -> Option<minimium_world::world::Position3Mm> {
        let can_fly = self
            .game
            .player_movement_policies
            .get(&id)
            .is_some_and(|policy| policy.can_fly);
        if !km.is_finite()
            || !(0.0..=10000.0).contains(&km)
            || !self.game.worlds.can_map_teleport(id, context)
            || !can_fly
        {
            return None;
        }
        let manifest = self.game.worlds.manifest_for_player(id)?;
        let definition = manifest.planet()?;
        let instance = self.game.worlds.instance_for_player(id)?;
        let position = self
            .game
            .worlds
            .get(instance)?
            .world
            .network_snapshot()
            .position_mm(id)?
            .0
            .map(|v| v as f64);
        let length = position.iter().map(|v| v.powi(2)).sum::<f64>().sqrt();
        if length < 1.0 {
            return None;
        }
        let direction = position.map(|v| (v / length) as f32);
        let ground = minimium_procgen::planet_collision_position_at_direction_with_geography(
            manifest,
            direction,
            self.prepared_geography.clone(),
        )?;
        let radius = ground
            .iter()
            .map(|v| (*v as f64).powi(2))
            .sum::<f64>()
            .sqrt();
        // Match optical/climate elevation units on compressed Earth relief.
        let scale = if definition.mode == 1 {
            f64::from(definition.height_scale_permille) / 1000.0
        } else {
            1.0
        };
        let target_radius = radius + f64::from(km) * 1_000_000.0 * scale;
        let position_mm = minimium_world::world::Position3Mm(std::array::from_fn(|a| {
            (position[a] / length * target_radius).round() as i64
        }));
        if position_mm
            .0
            .iter()
            .any(|v| v.unsigned_abs() > 7_000_000_000)
        {
            return None;
        }
        let world = &mut self.game.worlds.get_mut(instance)?.world;
        if !world.set_position(id, position_mm.0.map(|v| v as f32 / 1000.0))
            || !world.restore_position_mm(id, position_mm)
        {
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
                    "server.altitude_save_failed",
                    &[("player_id", id.to_string()), ("reason", error.to_string())],
                );
            }
        }
        if let Some(input) = self.game.active_inputs.get_mut(&id) {
            input.direction = [0.0; 2];
            input.vertical = 0.0;
            input.jump = false;
        }
        self.game.input_last_received.insert(id, Instant::now());
        self.scheduler.next_snapshot = Instant::now();
        Some(position_mm)
    }
}
