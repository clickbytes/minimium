//! Exact world-space tool ray against the currently installed collision image.
use super::{collision_index::CollisionIndex, world_state::World};
use minimium_core::ids::PlayerId;
use minimium_world::{
    terrain::{SurfaceHit, SurfaceTarget, TerrainChunkKey, TerrainMaterial, TerrainRevision},
    world::{Aabb, WorldInstanceId},
};

/// Position and radial up vector for a remote player capsule that can block a tool ray.
#[derive(Clone, Copy, Debug)]
pub struct ToolRayCapsule {
    pub feet_m: [f64; 3],
    pub up: [f64; 3],
}

impl CollisionIndex {
    /// Returns the nearest installed collider, surface or supplied capsule hit.
    /// This is the read-only ray used by authoritative tool admission and client previews.
    pub fn tool_surface_hit<I>(
        &self,
        world: WorldInstanceId,
        origin: [f64; 3],
        direction: [f64; 3],
        reach: f64,
        blockers: I,
    ) -> Option<SurfaceHit>
    where
        I: IntoIterator<Item = ToolRayCapsule>,
    {
        let length = dot(direction, direction).sqrt();
        if !origin.iter().all(|value| value.is_finite())
            || !length.is_finite()
            || length < 1.0e-9
            || !(0.0..=3.0).contains(&reach)
        {
            return None;
        }
        let direction = direction.map(|value| value / length);
        let mut found = None;
        trace(self, world, origin, direction, reach, &mut found);
        for blocker in blockers {
            if !blocker.feet_m.iter().all(|value| value.is_finite())
                || !blocker.up.iter().all(|value| value.is_finite())
            {
                continue;
            }
            let up_length = dot(blocker.up, blocker.up).sqrt();
            if !up_length.is_finite() || up_length < 1.0e-9 {
                continue;
            }
            let up = blocker.up.map(|value| value / up_length);
            let radius = f64::from(super::PLAYER_RADIUS);
            let feet = blocker.feet_m;
            let bottom = std::array::from_fn(|axis| feet[axis] + up[axis] * radius);
            let top_offset = f64::from(super::PLAYER_HEIGHT) - radius;
            let top = std::array::from_fn(|axis| feet[axis] + up[axis] * top_offset);
            if let Some(distance) =
                capsule_distance(origin, direction, limit(&found, reach), bottom, top, radius)
            {
                found = Some(hit(
                    world, origin, direction, distance, [0.0; 3], false, false,
                ));
            }
        }
        found
    }
}

impl World {
    pub fn tool_surface_hit(
        &self,
        world: WorldInstanceId,
        actor: PlayerId,
        origin: [f64; 3],
        direction: [f64; 3],
        reach: f64,
    ) -> Option<SurfaceHit> {
        let gravity = self.gravity();
        let blockers =
            self.players
                .iter()
                .filter(|(id, _)| **id != actor)
                .map(move |(_, player)| ToolRayCapsule {
                    feet_m: player.position_mm.map_or_else(
                        || player.position.map(f64::from),
                        |position| position.0.map(|value| value as f64 / 1000.0),
                    ),
                    up: minimium_world::world::character_frame(gravity, player.position)
                        .0
                        .map(f64::from),
                });
        self.spatial_geometry
            .movement
            .tool_surface_hit(world, origin, direction, reach, blockers)
    }
}

fn trace(
    index: &CollisionIndex,
    world: WorldInstanceId,
    origin: [f64; 3],
    direction: [f64; 3],
    reach: f64,
    found: &mut Option<SurfaceHit>,
) {
    if index
        .bounds
        .is_some_and(|b| !intersects(b, origin, direction, reach))
    {
        return;
    }
    for (bounds, part) in &index.partitions {
        if intersects(*bounds, origin, direction, limit(found, reach)) {
            trace(part, world, origin, direction, reach, found);
        }
    }
    for b in &index.colliders {
        if let Some(distance) = box_distance(
            origin,
            direction,
            b.min.map(f64::from),
            b.max.map(f64::from),
            limit(found, reach),
        ) {
            *found = Some(hit(
                world, origin, direction, distance, [0.0; 3], false, false,
            ));
        }
    }
    for triangle in &index.surfaces {
        let vertices = triangle
            .precise_vertices
            .map_or(triangle.vertices.map(|p| p.map(f64::from)), |v| {
                v.map(|p| p.map(|x| x as f64 / 1000.0))
            });
        let a = sub(vertices[1], vertices[0]);
        let b = sub(vertices[2], vertices[0]);
        let h = cross(direction, b);
        let det = dot(a, h);
        if det.abs() < 1.0e-12 {
            continue;
        }
        let s = sub(origin, vertices[0]);
        let u = dot(s, h) / det;
        if !(0.0..=1.0).contains(&u) {
            continue;
        }
        let q = cross(s, a);
        let v = dot(direction, q) / det;
        if v < 0.0 || u + v > 1.0 {
            continue;
        }
        let distance = dot(b, q) / det;
        if !(0.0..=limit(found, reach)).contains(&distance) {
            continue;
        }
        let normal = cross(a, b);
        let normal_length = dot(normal, normal).sqrt();
        if normal_length < 1.0e-12 {
            continue;
        }
        let rock = triangle
            .surface
            .is_some_and(|s| s.iter().map(|v| v.coverage[4]).sum::<f32>() > 1.5);
        *found = Some(hit(
            world,
            origin,
            direction,
            distance,
            normal.map(|v| v / normal_length),
            triangle.surface.is_some(),
            rock,
        ));
    }
}

fn hit(
    world: WorldInstanceId,
    origin: [f64; 3],
    direction: [f64; 3],
    distance: f64,
    normal: [f64; 3],
    terrain: bool,
    rock: bool,
) -> SurfaceHit {
    let position = std::array::from_fn(|i| origin[i] + direction[i] * distance);
    SurfaceHit {
        position_m: position,
        normal,
        distance_m: distance,
        target: if terrain {
            TerrainChunkKey::at(world, position)
                .map_or(SurfaceTarget::StaticGeometry, SurfaceTarget::Terrain)
        } else {
            SurfaceTarget::StaticGeometry
        },
        material: if rock {
            TerrainMaterial::Rock
        } else {
            TerrainMaterial::Soil
        },
        revision: TerrainRevision(0),
    }
}

fn limit(found: &Option<SurfaceHit>, reach: f64) -> f64 {
    found.map_or(reach, |h| h.distance_m)
}
fn intersects(b: Aabb, o: [f64; 3], d: [f64; 3], reach: f64) -> bool {
    box_distance(
        o,
        d,
        b.min.map(|v| f64::from(v) - 0.5),
        b.max.map(|v| f64::from(v) + 0.5),
        reach,
    )
    .is_some()
}
fn box_distance(o: [f64; 3], d: [f64; 3], lo: [f64; 3], hi: [f64; 3], reach: f64) -> Option<f64> {
    let mut start: f64 = 0.0;
    let mut end = reach;
    for axis in 0..3 {
        if d[axis].abs() < 1.0e-12 {
            if o[axis] < lo[axis] || o[axis] > hi[axis] {
                return None;
            }
        } else {
            let a = (lo[axis] - o[axis]) / d[axis];
            let b = (hi[axis] - o[axis]) / d[axis];
            start = start.max(a.min(b));
            end = end.min(a.max(b));
            if start > end {
                return None;
            }
        }
    }
    Some(start)
}
fn capsule_distance(
    origin: [f64; 3],
    direction: [f64; 3],
    reach: f64,
    start: [f64; 3],
    end: [f64; 3],
    radius: f64,
) -> Option<f64> {
    let distance = |t: f64| {
        let point = std::array::from_fn(|i| origin[i] + direction[i] * t);
        let axis = sub(end, start);
        let offset = sub(point, start);
        let denominator = dot(axis, axis);
        let u = if denominator > 1.0e-12 {
            (dot(offset, axis) / denominator).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let closest = std::array::from_fn(|i| start[i] + axis[i] * u);
        dot(sub(point, closest), sub(point, closest))
    };
    if distance(0.0) <= radius * radius {
        return Some(0.0);
    }
    let (mut left, mut right) = (0.0, reach);
    for _ in 0..48 {
        let a = left + (right - left) / 3.0;
        let b = right - (right - left) / 3.0;
        if distance(a) <= distance(b) {
            right = b;
        } else {
            left = a;
        }
    }
    let closest = (left + right) * 0.5;
    if distance(closest) > radius * radius {
        return None;
    }
    let (mut outside, mut inside) = (0.0, closest);
    for _ in 0..48 {
        let middle = (outside + inside) * 0.5;
        if distance(middle) <= radius * radius {
            inside = middle;
        } else {
            outside = middle;
        }
    }
    Some(inside)
}
fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
