//! Immutable collision partitions let streamed regions retain their built indexes.
use super::{
    collision_index::CollisionIndex, collision_queries::CollisionQuery, raycast::MapRayQuery,
};
use minimium_world::world::Aabb;
use std::sync::Arc;

impl CollisionIndex {
    pub fn from_partitions(parts: Vec<Arc<CollisionIndex>>) -> Self {
        let mut result = Self::default();
        for index in parts {
            let Some(bounds) = index.bounds else {
                continue;
            };
            result.include_bounds(bounds);
            let lo = grid(bounds.min);
            let hi = grid(bounds.max);
            let n = result.partitions.len();
            let volume = (0..3)
                .map(|i| i64::from(hi[i]) - i64::from(lo[i]) + 1)
                .fold(1_i64, i64::saturating_mul);
            if volume > 512 {
                result.large_partitions.push(n);
            } else {
                for x in lo[0]..=hi[0] {
                    for y in lo[1]..=hi[1] {
                        for z in lo[2]..=hi[2] {
                            result.partition_cells.entry([x, y, z]).or_default().push(n);
                        }
                    }
                }
            }
            result.partitions.push((bounds, index));
        }
        result
    }

    pub(super) fn nearby_partitions(&self, center: [f32; 3], reach: f32, out: &mut Vec<usize>) {
        out.clear();
        let lo = grid(center.map(|x| x - reach));
        let hi = grid(center.map(|x| x + reach));
        let volume = (0..3)
            .map(|i| i64::from(hi[i]) - i64::from(lo[i]) + 1)
            .fold(1_i64, i64::saturating_mul);
        if volume > 512 || !reach.is_finite() {
            out.extend(0..self.partitions.len());
        } else {
            out.extend_from_slice(&self.large_partitions);
            for x in lo[0]..=hi[0] {
                for y in lo[1]..=hi[1] {
                    for z in lo[2]..=hi[2] {
                        if let Some(indices) = self.partition_cells.get(&[x, y, z]) {
                            out.extend_from_slice(indices);
                        }
                    }
                }
            }
            out.sort_unstable();
            out.dedup();
        }
        out.retain(|&i| {
            let b = self.partitions[i].0;
            (0..3).all(|axis| {
                center[axis] + reach >= b.min[axis] && center[axis] - reach <= b.max[axis]
            })
        });
    }

    pub(super) fn all_colliders(&self) -> impl Iterator<Item = &Aabb> {
        self.colliders
            .iter()
            .chain(self.partitions.iter().flat_map(|(_, p)| p.colliders.iter()))
    }
}
fn grid(p: [f32; 3]) -> [i32; 3] {
    p.map(|x| (x / 512.0).floor() as i32)
}

impl CollisionQuery {
    pub(super) fn fill_partitions(
        &mut self,
        index: &CollisionIndex,
        state: super::rules::CharacterState,
        dt: f32,
        speed: f32,
    ) -> super::CollisionQueryStats {
        self.candidates.clear();
        self.surface_candidates.clear();
        let reach = speed.max(50.0) * dt + 4.0;
        index.nearby_partitions(state.position, reach, &mut self.partition_candidates);
        let query = self.partition_query.get_or_insert_with(Default::default);
        let mut stats = super::CollisionQueryStats::default();
        for &part in &self.partition_candidates {
            stats.merge(query.fill_for_speed(&index.partitions[part].1, state, dt, speed));
            self.candidates.extend_from_slice(&query.candidates);
            self.surface_candidates
                .extend_from_slice(&query.surface_candidates);
        }
        stats
    }
}

impl MapRayQuery {
    pub(super) fn fill_partitions(
        &mut self,
        index: &CollisionIndex,
        origin: [f32; 3],
        direction: [f32; 3],
        reach: f32,
    ) {
        let center = std::array::from_fn(|i| origin[i] + direction[i] * reach * 0.5);
        index.nearby_partitions(center, reach * 0.5 + 1.0, &mut self.partition_candidates);
        self.partition_hit = None;
        let query = self.partition_query.get_or_insert_with(Default::default);
        for &part in &self.partition_candidates {
            let child = &index.partitions[part].1;
            query.fill(child, origin, direction, reach);
            if let Some(hit) = query.nearest_blocker(child, origin, direction, reach) {
                self.partition_hit = Some(self.partition_hit.map_or(hit, |old| old.min(hit)));
            }
        }
    }
}
