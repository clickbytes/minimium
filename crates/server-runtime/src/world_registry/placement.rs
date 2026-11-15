use super::WorldRegistry;
use minimium_world::{
    terrain::TerrainRead,
    world::{Position3Mm, WorldInstanceId},
};
use std::collections::HashMap;

pub(crate) type TerrainReads<'a> = HashMap<WorldInstanceId, &'a dyn TerrainRead>;

/// Conservative read-only check for a standing 0.3 m radius, 1.7 m capsule.
/// Missing terrain samples fail closed; planetary placement may never fall back
/// to a heightfield or sea-floor assumption.
pub(crate) fn terrain_capsule_clear(read: &dyn TerrainRead, feet: Position3Mm) -> bool {
    let feet = feet.0.map(|value| value as f64 / 1000.0);
    let radius = 0.3;
    let up = normalize(feet);
    let Some(up) = up else { return false };
    let start = add(feet, scale(up, radius));
    let end = add(feet, scale(up, 1.4));
    let length = distance(start, end);
    let steps = (length / 0.075).ceil().clamp(1.0, 20.0) as usize;
    (0..=steps).all(|index| {
        let t = index as f64 / steps as f64;
        let point = std::array::from_fn(|axis| start[axis] + (end[axis] - start[axis]) * t);
        read.sample(point).is_some_and(|sample| {
            sample.distance_m.is_finite() && sample.distance_m >= radius - 0.005
        })
    })
}

impl WorldRegistry {
    pub(crate) fn transfer_destination_clear(
        &self,
        instance: WorldInstanceId,
        destination: Position3Mm,
        reads: &TerrainReads<'_>,
    ) -> bool {
        let Some(owner) = self.get(instance) else {
            return false;
        };
        if !owner.map.can_place_player_with_gravity(
            destination.0.map(|value| value as f32 / 1000.0),
            owner.world.gravity(),
        ) {
            return false;
        }
        if owner.manifest.planet().is_some() {
            return reads
                .get(&instance)
                .is_some_and(|terrain| terrain_capsule_clear(*terrain, destination));
        }
        true
    }
}

fn normalize(value: [f64; 3]) -> Option<[f64; 3]> {
    let length = distance(value, [0.0; 3]);
    (length.is_finite() && length > 1.0).then(|| scale(value, length.recip()))
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| a[axis] + b[axis])
}

fn scale(value: [f64; 3], factor: f64) -> [f64; 3] {
    value.map(|component| component * factor)
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter()
        .zip(b)
        .map(|(left, right)| (left - right).powi(2))
        .sum::<f64>()
        .sqrt()
}
