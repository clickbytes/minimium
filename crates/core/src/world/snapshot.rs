use super::collision_index::Cell;
use super::geometry::cell_for;
use super::model::Position3Mm;
use super::rules::{CharacterGravity, CharacterState, Player};
use super::VISIBILITY_RADIUS;
use crate::ids::PlayerId;
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::sync::Arc;

#[derive(Clone)]
pub struct WorldSnapshot {
    pub(super) players: Arc<HashMap<PlayerId, Player>>,
    pub(super) cells: Arc<HashMap<Cell, Vec<PlayerId>>>,
    pub(super) gravity: CharacterGravity,
}

#[derive(Clone, Copy)]
pub(super) struct NearbyCandidate<'a> {
    pub(super) player: &'a Player,
    pub(super) distance_squared: f32,
}

impl WorldSnapshot {
    pub fn position(&self, entity: PlayerId) -> Option<[f32; 3]> {
        self.players.get(&entity).map(|player| player.position)
    }

    pub fn position_mm(&self, entity: PlayerId) -> Option<Position3Mm> {
        self.players.get(&entity).map(|player| {
            player.position_mm.unwrap_or_else(|| {
                Position3Mm(
                    player
                        .position
                        .map(|value| (f64::from(value) * 1000.0).round() as i64),
                )
            })
        })
    }

    pub fn character_state(&self, entity: PlayerId) -> Option<CharacterState> {
        self.players.get(&entity).map(|player| CharacterState {
            position: player.position,
            velocity: player.velocity,
            grounded: player.grounded,
            gravity: self.gravity,
        })
    }

    pub fn append_nearby(&self, recipient: PlayerId, limit: usize, output: &mut String) {
        let mut append = |player: &Player| {
            let _ = write!(
                output,
                " {} {} {:.2} {:.2} {:.2}",
                player.id, player.name, player.position[0], player.position[1], player.position[2]
            );
        };
        if limit <= 16 {
            self.visit_nearby::<16>(recipient, limit, &mut append);
        } else {
            for player in self.nearby_players(recipient, limit) {
                append(&player);
            }
        }
    }

    pub fn visit_nearby<const CAPACITY: usize>(
        &self,
        recipient: PlayerId,
        limit: usize,
        mut visit: impl FnMut(&Player),
    ) {
        let selection_limit = limit.min(CAPACITY);
        if selection_limit == 0 {
            return;
        }

        let mut nearest: [Option<NearbyCandidate<'_>>; CAPACITY] = [None; CAPACITY];
        let mut nearest_count = 0;
        self.for_each_nearby_candidate(recipient, |player, distance_squared| {
            let candidate = NearbyCandidate {
                player,
                distance_squared,
            };
            let insert_at = if nearest_count < selection_limit {
                let index = nearest_count;
                nearest_count += 1;
                index
            } else {
                let last = nearest[selection_limit - 1].expect("nearest entries stay packed");
                if nearby_candidate_precedes(candidate, last) {
                    selection_limit - 1
                } else {
                    return;
                }
            };
            nearest[insert_at] = Some(candidate);
            let mut index = insert_at;
            while index > 0 {
                let previous = nearest[index - 1].expect("nearest entries stay packed");
                let current = nearest[index].expect("inserted candidate is present");
                if !nearby_candidate_precedes(current, previous) {
                    break;
                }
                nearest[index] = Some(previous);
                nearest[index - 1] = Some(current);
                index -= 1;
            }
        });

        for candidate in nearest.into_iter().take(nearest_count).flatten() {
            visit(candidate.player);
        }
    }

    pub fn nearby_players(&self, recipient: PlayerId, limit: usize) -> Vec<Player> {
        if limit == 0 {
            return Vec::new();
        }
        let mut candidates = Vec::with_capacity(limit.min(self.players.len()));
        self.for_each_nearby_candidate(recipient, |player, distance_squared| {
            candidates.push((distance_squared, player.id, player));
        });
        candidates.sort_unstable_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        candidates
            .into_iter()
            .take(limit)
            .map(|(_, _, player)| player.clone())
            .collect()
    }

    fn for_each_nearby_candidate<'a>(
        &'a self,
        recipient: PlayerId,
        mut visit: impl FnMut(&'a Player, f32),
    ) {
        let Some(origin) = self.players.get(&recipient) else {
            return;
        };
        let minimum = cell_for([
            origin.position[0] - VISIBILITY_RADIUS,
            origin.position[1] - VISIBILITY_RADIUS,
            origin.position[2] - VISIBILITY_RADIUS,
        ]);
        let maximum = cell_for([
            origin.position[0] + VISIBILITY_RADIUS,
            origin.position[1] + VISIBILITY_RADIUS,
            origin.position[2] + VISIBILITY_RADIUS,
        ]);
        let radius_squared = VISIBILITY_RADIUS * VISIBILITY_RADIUS;
        let axis_cell_count = |minimum: i32, maximum: i32| {
            usize::try_from(i64::from(maximum) - i64::from(minimum) + 1).unwrap_or(usize::MAX)
        };
        let query_cell_count = axis_cell_count(minimum.0, maximum.0)
            .saturating_mul(axis_cell_count(minimum.1, maximum.1))
            .saturating_mul(axis_cell_count(minimum.2, maximum.2));
        let mut inspect_ids = |ids: &[PlayerId]| {
            for id in ids {
                if *id == recipient {
                    continue;
                }
                let Some(player) = self.players.get(id) else {
                    continue;
                };
                let distance_squared = (0..3)
                    .map(|axis| {
                        let delta = player.position[axis] - origin.position[axis];
                        delta * delta
                    })
                    .sum::<f32>();
                if distance_squared <= radius_squared {
                    visit(player, distance_squared);
                }
            }
        };

        if self.cells.len() < query_cell_count {
            for (Cell(x, y, z), ids) in self.cells.iter() {
                if *x >= minimum.0
                    && *x <= maximum.0
                    && *y >= minimum.1
                    && *y <= maximum.1
                    && *z >= minimum.2
                    && *z <= maximum.2
                {
                    inspect_ids(ids);
                }
            }
        } else {
            for z in minimum.2..=maximum.2 {
                for y in minimum.1..=maximum.1 {
                    for x in minimum.0..=maximum.0 {
                        if let Some(ids) = self.cells.get(&Cell(x, y, z)) {
                            inspect_ids(ids);
                        }
                    }
                }
            }
        }
    }
}

pub(super) fn nearby_candidate_precedes(
    left: NearbyCandidate<'_>,
    right: NearbyCandidate<'_>,
) -> bool {
    left.distance_squared
        .total_cmp(&right.distance_squared)
        .then_with(|| left.player.id.cmp(&right.player.id))
        .is_lt()
}
