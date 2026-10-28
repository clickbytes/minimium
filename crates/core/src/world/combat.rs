use super::collision_index::Aabb;
use super::geometry::update_cell_index;
use super::raycast::ray_aabb_distance;
use super::rules::ShotOutcome;
use super::world_state::World;
use super::{HITSCAN_DAMAGE, HITSCAN_RANGE, PLAYER_HEIGHT, PLAYER_MAX_HEALTH, PLAYER_RADIUS};
use std::sync::Arc;

impl World {
    pub fn fire_hitscan(&mut self, attacker_id: u64, direction: [f32; 3]) -> Option<ShotOutcome> {
        if direction.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let length_squared = direction.iter().map(|value| value * value).sum::<f32>();
        if !length_squared.is_finite() || length_squared <= 1.0e-8 {
            return None;
        }
        let inverse_length = length_squared.sqrt().recip();
        let direction = direction.map(|value| value * inverse_length);
        let World {
            players,
            cells,
            player_cells,
            collision_index,
            combat_player_query,
            combat_map_query,
            spawn,
            ..
        } = self;
        let players = Arc::make_mut(players);
        let attacker = players.get(&attacker_id)?;
        let origin = [
            attacker.position[0],
            attacker.position[1] + PLAYER_HEIGHT * 0.9,
            attacker.position[2],
        ];
        combat_player_query.fill(cells, origin, direction, HITSCAN_RANGE);
        combat_map_query.fill(collision_index, origin, direction, HITSCAN_RANGE);

        let mut nearest_player: Option<(u64, f32)> = None;
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
            let Some(distance) = ray_aabb_distance(origin, direction, bounds, HITSCAN_RANGE) else {
                continue;
            };
            if nearest_player.is_none_or(|(nearest_id, nearest_distance)| {
                distance.total_cmp(&nearest_distance).is_lt()
                    || (distance == nearest_distance && *candidate_id < nearest_id)
            }) {
                nearest_player = Some((*candidate_id, distance));
            }
        }

        let nearest_blocker =
            combat_map_query.nearest_blocker(collision_index, origin, direction, HITSCAN_RANGE);
        if nearest_player.is_none_or(|(_, player_distance)| {
            nearest_blocker.is_some_and(|blocker_distance| blocker_distance <= player_distance)
        }) {
            return Some(ShotOutcome {
                target_id: None,
                remaining_health: None,
                killed: false,
            });
        }
        let (target_id, _) = nearest_player?;
        let target = players.get_mut(&target_id)?;
        let remaining_health = target.health.saturating_sub(HITSCAN_DAMAGE);
        let killed = remaining_health == 0;
        if killed {
            target.health = PLAYER_MAX_HEALTH;
            target.position = *spawn;
            target.velocity = [0.0; 3];
            target.grounded = false;
            update_cell_index(
                Arc::make_mut(cells),
                Arc::make_mut(player_cells),
                target_id,
                *spawn,
            );
        } else {
            target.health = remaining_health;
        }
        Some(ShotOutcome {
            target_id: Some(target_id),
            remaining_health: Some(remaining_health),
            killed,
        })
    }
}
