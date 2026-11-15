use super::raycast::ray_aabb_distance;
use super::rules::ShotOutcome;
use super::spatial::update_cell_index;
use super::spatial_geometry::CombatCoveragePlan;
use super::world_state::World;
use super::{PLAYER_HEIGHT, PLAYER_MAX_HEALTH, PLAYER_RADIUS};
use minimium_core::ids::PlayerId;
use minimium_world::world::Aabb;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShotReadinessError {
    NotReady,
}

/// Game-supplied parameters for the shared hitscan combat rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CombatSettings {
    /// Damage applied to a hit player.
    pub damage: u16,
    /// Maximum ray distance in world units.
    pub range: f32,
}

impl World {
    pub fn fire_hitscan(
        &mut self,
        attacker_id: PlayerId,
        direction: [f32; 3],
        settings: CombatSettings,
        coverage: &CombatCoveragePlan,
    ) -> Result<Option<ShotOutcome>, ShotReadinessError> {
        self.fire_hitscan_inner(attacker_id, direction, settings, coverage, false)
    }

    /// Applies damage without moving a killed player. The owner lane must place
    /// the player at a validated current-geometry respawn point before publish.
    pub fn fire_hitscan_deferred_respawn(
        &mut self,
        attacker_id: PlayerId,
        direction: [f32; 3],
        settings: CombatSettings,
        coverage: &CombatCoveragePlan,
    ) -> Result<Option<ShotOutcome>, ShotReadinessError> {
        self.fire_hitscan_inner(attacker_id, direction, settings, coverage, true)
    }

    fn fire_hitscan_inner(
        &mut self,
        attacker_id: PlayerId,
        direction: [f32; 3],
        settings: CombatSettings,
        coverage: &CombatCoveragePlan,
        defer_respawn: bool,
    ) -> Result<Option<ShotOutcome>, ShotReadinessError> {
        if settings.damage == 0 || !settings.range.is_finite() || settings.range <= 0.0 {
            return Ok(None);
        }
        if direction.iter().any(|value| !value.is_finite()) {
            return Ok(None);
        }
        let length_squared = direction.iter().map(|value| value * value).sum::<f32>();
        if !length_squared.is_finite() || length_squared <= 1.0e-8 {
            return Ok(None);
        }
        let inverse_length = length_squared.sqrt().recip();
        let direction = direction.map(|value| value * inverse_length);
        let World {
            players,
            cells,
            player_cells,
            spatial_geometry,
            combat_player_query,
            combat_map_query,
            spawn,
            ..
        } = self;
        let Some(attacker) = players.get(&attacker_id) else {
            return Ok(None);
        };
        let origin = [
            attacker.position[0],
            attacker.position[1] + PLAYER_HEIGHT * 0.9,
            attacker.position[2],
        ];
        if !coverage.matches(spatial_geometry.revision, origin, direction, settings.range) {
            return Err(ShotReadinessError::NotReady);
        }
        let players = Arc::make_mut(players);
        combat_player_query.fill(cells, origin, direction, settings.range);

        let mut nearest_player: Option<(PlayerId, f32)> = None;
        for candidate_id in &combat_player_query.candidates {
            if *candidate_id == attacker_id {
                continue;
            }
            let Some(candidate) = players.get(candidate_id) else {
                continue;
            };
            let bounds = Aabb {
                min: [
                    candidate.position[0] - PLAYER_RADIUS,
                    candidate.position[1],
                    candidate.position[2] - PLAYER_RADIUS,
                ],
                max: [
                    candidate.position[0] + PLAYER_RADIUS,
                    candidate.position[1] + PLAYER_HEIGHT,
                    candidate.position[2] + PLAYER_RADIUS,
                ],
            };
            let Some(distance) = ray_aabb_distance(origin, direction, bounds, settings.range)
            else {
                continue;
            };
            if nearest_player.is_none_or(|(nearest_id, nearest_distance)| {
                distance.total_cmp(&nearest_distance).is_lt()
                    || (distance == nearest_distance && *candidate_id < nearest_id)
            }) {
                nearest_player = Some((*candidate_id, distance));
            }
        }

        let sector_for_point = |point: [f32; 3]| {
            let sector = minimium_world::world::PlanetSectorKey::from_direction_with_count(
                point,
                coverage.sector_count,
            );
            minimium_world::world::SectorKey::planet(
                sector.face(),
                sector.coordinates().0,
                sector.coordinates().1,
            )
        };
        combat_map_query.fill(
            &spatial_geometry.combat_baseline,
            origin,
            direction,
            settings.range,
        );
        let mut nearest_blocker = combat_map_query.nearest_blocker_where(
            &spatial_geometry.combat_baseline,
            origin,
            direction,
            settings.range,
            |point| {
                sector_for_point(point).is_none_or(|sector| !coverage.sectors.contains(&sector))
            },
        );
        for sector in &coverage.sectors {
            let Some(index) = spatial_geometry.combat_replacements.get(sector) else {
                return Err(ShotReadinessError::NotReady);
            };
            combat_map_query.fill(index, origin, direction, settings.range);
            if let Some(hit) = combat_map_query.nearest_blocker_where(
                index,
                origin,
                direction,
                settings.range,
                |point| sector_for_point(point) == Some(*sector),
            ) {
                nearest_blocker = Some(nearest_blocker.map_or(hit, |old: f32| old.min(hit)));
            }
        }
        if nearest_player.is_none_or(|(_, player_distance)| {
            nearest_blocker.is_some_and(|blocker_distance| blocker_distance <= player_distance)
        }) {
            return Ok(Some(ShotOutcome {
                target_id: None,
                remaining_health: None,
                killed: false,
            }));
        }
        let Some((target_id, _)) = nearest_player else {
            return Ok(None);
        };
        let Some(target) = players.get_mut(&target_id) else {
            return Ok(None);
        };
        let remaining_health = target.health.saturating_sub(settings.damage);
        let killed = remaining_health == 0;
        if killed {
            target.health = PLAYER_MAX_HEALTH;
            target.velocity = [0.0; 3];
            target.grounded = false;
            if !defer_respawn {
                target.position = *spawn;
                update_cell_index(
                    Arc::make_mut(cells),
                    Arc::make_mut(player_cells),
                    target_id,
                    *spawn,
                );
            }
        } else {
            target.health = remaining_health;
        }
        Ok(Some(ShotOutcome {
            target_id: Some(target_id),
            remaining_health: Some(remaining_health),
            killed,
        }))
    }
}
