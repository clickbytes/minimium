use super::ServerRuntime;
use crate::{connection::queue_message, world_inputs::WorldEvent};
use minimium_core::ids::PlayerId;
use minimium_world::world::WorldInstanceId;

impl ServerRuntime {
    pub(super) fn apply_fire_input(
        &mut self,
        player: PlayerId,
        direction: [f32; 3],
    ) -> Option<WorldEvent> {
        if self.player_inventory(player).tool().is_some() {
            return None;
        }
        let instance = self.game.worlds.instance_for_player(player)?;
        let manifest = self.game.worlds.get(instance)?.manifest;
        let context = self.game.worlds.context_for_player(player)?;
        let player_state = self.game.worlds.get(instance)?.world.player(player)?;
        let origin = [
            player_state.position[0],
            player_state.position[1] + minimium_world::world::PLAYER_HEIGHT * 0.9,
            player_state.position[2],
        ];
        let keys = crate::sector_streaming::combat_ray_sectors(
            manifest,
            origin,
            direction,
            self.combat.range,
        );
        let Some(keys) = keys else {
            self.report_combat_not_ready(player, instance, "combat_sector_demand_limit");
            return None;
        };
        let sector_count = minimium_procgen::planet_sector_count(manifest);
        let coverage = self.game.worlds.get(instance)?.world.combat_coverage_plan(
            origin,
            direction,
            self.combat.range,
            sector_count,
            &keys,
        );
        let result = {
            let world = &mut self.game.worlds.get_mut(instance)?.world;
            if manifest.planet().is_some() {
                world.fire_hitscan_deferred_respawn(player, direction, self.combat, &coverage)
            } else {
                world.fire_hitscan(player, direction, self.combat, &coverage)
            }
        };
        let outcome = match result {
            Ok(Some(outcome)) => outcome,
            Ok(None) => return None,
            Err(minimium_simulation::ShotReadinessError::NotReady) => {
                let current: usize = self
                    .game
                    .combat_sector_demands
                    .values()
                    .map(|demand| demand.keys.len())
                    .sum();
                let replaced = self
                    .game
                    .combat_sector_demands
                    .get(&player)
                    .map_or(0, |demand| demand.keys.len());
                if current.saturating_sub(replaced).saturating_add(keys.len())
                    > crate::sector_streaming::MAX_PENDING_COMBAT_SECTORS
                {
                    self.report_combat_not_ready(player, instance, "combat_sector_demand_capacity");
                    return None;
                }
                self.game.combat_sector_demands.insert(
                    player,
                    crate::sector_streaming::CombatSectorDemand { context, keys },
                );
                self.report_combat_not_ready(player, instance, "combat_sector_not_ready");
                return None;
            }
        };
        self.game.combat_sector_demands.remove(&player);
        self.metrics.shots_fired = self.metrics.shots_fired.saturating_add(1);
        if outcome.killed && manifest.planet().is_some() {
            if let Some(respawn_mm) = super::gameplay_terrain::safe_planet_spawn(
                &self.game.worlds,
                instance,
                self.prepared_geography.as_ref(),
                self.excavation.soil_depth_m(),
            ) {
                let respawn = respawn_mm.0.map(|value| value as f32 / 1000.0);
                if let (Some(target), Some(world)) =
                    (outcome.target_id, self.game.worlds.get_mut(instance))
                {
                    world.world.set_position(target, respawn);
                    world.world.restore_position_mm(target, respawn_mm);
                }
            }
        }
        let target_health = outcome.target_id.and_then(|target| {
            self.game
                .worlds
                .get(instance)
                .and_then(|owner| owner.world.player_health(target))
        });
        if outcome.target_id.is_some() {
            self.metrics.shots_hit = self.metrics.shots_hit.saturating_add(1);
            if outcome.killed {
                self.metrics.player_kills = self.metrics.player_kills.saturating_add(1);
            }
        }
        Some(WorldEvent::Shot {
            instance,
            shooter: player,
            outcome,
            target_health,
        })
    }

    fn report_combat_not_ready(
        &mut self,
        player: PlayerId,
        instance: WorldInstanceId,
        code: &'static str,
    ) {
        crate::diagnostics::emit(
            &self.world_logger,
            minimium_diagnostics::Level::Warn,
            code,
            &[
                ("world", instance.to_string()),
                ("player", player.to_string()),
            ],
        );
        if let Some(index) = self
            .connections
            .client_indices_by_player
            .get(&player)
            .copied()
        {
            if let Some(client) = self.connections.clients.get_mut(index) {
                if !queue_message(
                    client,
                    &mut self.metrics,
                    minimium_protocol::ServerMessage::Error(
                        minimium_protocol::ServerErrorCode::Busy,
                    ),
                ) {
                    self.metrics.dropped_messages = self.metrics.dropped_messages.saturating_add(1);
                }
            }
        }
    }
}
