use crate::{generate_planet_sector_with_geography, planet_sector_count};
use minimium_world::world::{PlanetSectorKey, WorldManifest};

/// Intersect a globe click with the same triangles used for collision.
/// The sampled elevation alone can sit below a triangle on steep slopes.
pub fn planet_collision_position_at_direction(
    manifest: WorldManifest,
    direction: [f32; 3],
) -> Option<[i64; 3]> {
    planet_collision_position_at_direction_with_geography(manifest, direction, None)
}

pub fn planet_collision_position_at_direction_with_geography(
    manifest: WorldManifest,
    direction: [f32; 3],
    geography: impl Into<crate::TerrainInputs>,
) -> Option<[i64; 3]> {
    planet_position_at_direction(manifest, direction, geography.into(), false)
}

/// Returns the exposed terrain hit only when a 0.3 m radius, 1.7 m tall
/// capsule has clearance 2 m above it in the current sector and its neighbors.
pub fn planet_safe_spawn_position_at_direction_with_geography(
    manifest: WorldManifest,
    direction: [f32; 3],
    geography: impl Into<crate::TerrainInputs>,
) -> Option<[i64; 3]> {
    planet_position_at_direction(manifest, direction, geography.into(), true)
}

fn planet_position_at_direction(
    manifest: WorldManifest,
    direction: [f32; 3],
    inputs: crate::TerrainInputs,
    safe_spawn: bool,
) -> Option<[i64; 3]> {
    let count = planet_sector_count(manifest);
    let key = PlanetSectorKey::from_direction_with_count(direction, count);
    let mut terrain = if inputs.volumes.is_some() {
        Some(
            crate::PlanetTerrain::new(manifest.generation(), crate::planet_surface_for(manifest))?
                .with_inputs(inputs.clone()),
        )
    } else {
        None
    };
    if let Some(terrain) = &mut terrain {
        if let Some(definition) = manifest.planet() {
            *terrain = terrain.clone().with_definition(definition);
        }
    }
    let neighbors = if safe_spawn || terrain.is_some() {
        bounded_neighbors(key, direction, count)
    } else {
        Vec::new()
    };
    let mut bases = std::collections::BTreeMap::new();
    bases.insert(
        key,
        generate_planet_sector_with_geography(manifest, key, inputs.clone()),
    );
    for adjacent in &neighbors {
        bases.insert(
            *adjacent,
            generate_planet_sector_with_geography(manifest, *adjacent, inputs.clone()),
        );
    }
    let contours: Vec<_> = bases
        .values()
        .flat_map(|geometry| geometry.triangles.iter().copied())
        .collect();
    let base = bases.get(&key)?;
    let geometry = if let Some(terrain) = &terrain {
        crate::integrate_volume_sector_with_contours(terrain, key, count, base, &contours, 1.5)
            .ok()?
    } else {
        base.clone()
    };
    let mut clearance_geometry = vec![geometry.clone()];
    if safe_spawn {
        for adjacent in neighbors {
            let adjacent_base = bases.get(&adjacent)?;
            let adjacent_geometry = if let Some(terrain) = &terrain {
                crate::integrate_volume_sector_with_contours(
                    terrain,
                    adjacent,
                    count,
                    adjacent_base,
                    &contours,
                    1.5,
                )
                .ok()?
            } else {
                adjacent_base.clone()
            };
            clearance_geometry.push(adjacent_geometry);
        }
    }
    let d = direction.map(f64::from);
    let mut outermost = None::<f64>;
    for triangle in geometry.triangles.iter().chain(&geometry.object_triangles) {
        if !triangle.solid {
            continue;
        }
        let vertices = triangle.precise_vertices?;
        let a = vertices[0].map(|value| value as f64);
        let b = vertices[1].map(|value| value as f64);
        let c = vertices[2].map(|value| value as f64);
        let ab = sub(b, a);
        let ac = sub(c, a);
        let normal = cross(ab, ac);
        let denominator = dot(normal, d);
        if denominator.abs() < 1.0e-9 {
            continue;
        }
        let distance = dot(normal, a) / denominator;
        if distance <= 0.0 {
            continue;
        }
        let relative = sub(d.map(|value| value * distance), a);
        let ab_ab = dot(ab, ab);
        let ab_ac = dot(ab, ac);
        let ac_ac = dot(ac, ac);
        let rel_ab = dot(relative, ab);
        let rel_ac = dot(relative, ac);
        let determinant = ab_ab * ac_ac - ab_ac * ab_ac;
        if determinant.abs() < 1.0 {
            continue;
        }
        let u = (rel_ab * ac_ac - rel_ac * ab_ac) / determinant;
        let v = (rel_ac * ab_ab - rel_ab * ab_ac) / determinant;
        if u >= -1.0e-5 && v >= -1.0e-5 && u + v <= 1.0 + 1.0e-5 {
            outermost = Some(outermost.map_or(distance, |old| old.max(distance)));
        }
    }
    outermost.and_then(|distance| {
        let hit_mm = d.map(|value| value * distance);
        let hit_m = hit_mm.map(|value| value / 1000.0);
        let direction_length = dot(d, d).sqrt();
        let up = d.map(|value| value / direction_length);
        let spawn_feet = std::array::from_fn(|axis| hit_m[axis] + up[axis] * 2.0);
        (!safe_spawn || capsule_clear(&clearance_geometry, spawn_feet))
            .then(|| hit_mm.map(|value| value.round() as i64))
    })
}

fn bounded_neighbors(
    center: PlanetSectorKey,
    direction: [f32; 3],
    count: u16,
) -> Vec<PlanetSectorKey> {
    let cardinal = center.neighbors_with_count(count);
    let mut candidates = vec![center];
    candidates.extend(cardinal.iter().copied());
    for (index, neighbor) in cardinal.iter().copied().enumerate() {
        let side_edges = if index < 2 { [2, 3] } else { [0, 1] };
        let diagonal = neighbor.neighbors_with_count(count);
        candidates.extend(side_edges.map(|edge| diagonal[edge]));
    }
    candidates.sort_unstable();
    candidates.dedup();
    candidates.retain(|key| *key != center);
    let target = direction.map(f64::from);
    let target_length = dot(target, target).sqrt().max(f64::EPSILON);
    let target = target.map(|value| value / target_length);
    candidates
        .sort_by(|a, b| center_dot(*b, count, target).total_cmp(&center_dot(*a, count, target)));
    candidates.truncate(8);
    candidates
}

fn center_dot(key: PlanetSectorKey, count: u16, target: [f64; 3]) -> f64 {
    let Some(sample) = key.cube_sample_with_count(1, 1, 2, count) else {
        return -1.0;
    };
    let center = sample.coordinates().map(f64::from);
    let length = dot(center, center).sqrt().max(f64::EPSILON);
    dot(center.map(|value| value / length), target)
}

fn capsule_clear(geometries: &[crate::SectorGeometry], feet: [f64; 3]) -> bool {
    let radius = 0.3;
    let up_length = dot(feet, feet).sqrt();
    if up_length <= 1.0 {
        return false;
    }
    let up = feet.map(|value| value / up_length);
    let start = std::array::from_fn(|axis| feet[axis] + up[axis] * radius);
    let end = std::array::from_fn(|axis| feet[axis] + up[axis] * 1.4);
    for geometry in geometries {
        for triangle in geometry.triangles.iter().chain(&geometry.object_triangles) {
            if !triangle.solid {
                continue;
            }
            let vertices = triangle.precise_vertices.map_or_else(
                || triangle.vertices.map(|p| p.map(f64::from)),
                |v| v.map(|p| p.map(|x| x as f64 / 1000.0)),
            );
            if segment_triangle_distance_squared(start, end, vertices) < radius * radius {
                return false;
            }
        }
        if geometry.boxes.iter().any(|map_box| {
            map_box.solid
                && segment_intersects_box(
                    start,
                    end,
                    map_box.center.map(f64::from),
                    map_box.size.map(|v| f64::from(v) * 0.5),
                    radius,
                )
        }) {
            return false;
        }
    }
    true
}

fn segment_triangle_distance_squared(
    start: [f64; 3],
    end: [f64; 3],
    triangle: [[f64; 3]; 3],
) -> f64 {
    if segment_intersects_triangle(start, end, triangle) {
        return 0.0;
    }
    let mut result = point_triangle_distance_squared(start, triangle)
        .min(point_triangle_distance_squared(end, triangle));
    for edge in 0..3 {
        result = result.min(segment_segment_distance_squared(
            start,
            end,
            triangle[edge],
            triangle[(edge + 1) % 3],
        ));
    }
    result
}

fn segment_intersects_triangle(start: [f64; 3], end: [f64; 3], triangle: [[f64; 3]; 3]) -> bool {
    let direction = sub(end, start);
    let edge1 = sub(triangle[1], triangle[0]);
    let edge2 = sub(triangle[2], triangle[0]);
    let h = cross(direction, edge2);
    let determinant = dot(edge1, h);
    if determinant.abs() < 1.0e-12 {
        return false;
    }
    let offset = sub(start, triangle[0]);
    let u = dot(offset, h) / determinant;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = cross(offset, edge1);
    let v = dot(direction, q) / determinant;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let t = dot(edge2, q) / determinant;
    (0.0..=1.0).contains(&t)
}

fn point_triangle_distance_squared(point: [f64; 3], triangle: [[f64; 3]; 3]) -> f64 {
    let edge1 = sub(triangle[1], triangle[0]);
    let edge2 = sub(triangle[2], triangle[0]);
    let normal = cross(edge1, edge2);
    let normal_length2 = dot(normal, normal);
    if normal_length2 <= 1.0e-18 {
        return (0..3)
            .map(|i| point_segment_distance_squared(point, triangle[i], triangle[(i + 1) % 3]))
            .fold(f64::INFINITY, f64::min);
    }
    let signed = dot(sub(point, triangle[0]), normal) / normal_length2;
    let projected = std::array::from_fn(|i| point[i] - normal[i] * signed);
    let relative = sub(projected, triangle[0]);
    let (d00, d01, d11) = (dot(edge1, edge1), dot(edge1, edge2), dot(edge2, edge2));
    let (d20, d21) = (dot(relative, edge1), dot(relative, edge2));
    let denominator = d00 * d11 - d01 * d01;
    if denominator.abs() > 1.0e-18 {
        let v = (d11 * d20 - d01 * d21) / denominator;
        let w = (d00 * d21 - d01 * d20) / denominator;
        if v >= 0.0 && w >= 0.0 && v + w <= 1.0 {
            return dot(sub(point, projected), sub(point, projected));
        }
    }
    (0..3)
        .map(|i| point_segment_distance_squared(point, triangle[i], triangle[(i + 1) % 3]))
        .fold(f64::INFINITY, f64::min)
}

fn point_segment_distance_squared(point: [f64; 3], start: [f64; 3], end: [f64; 3]) -> f64 {
    let segment = sub(end, start);
    let t = (dot(sub(point, start), segment) / dot(segment, segment).max(1.0e-18)).clamp(0.0, 1.0);
    let nearest = std::array::from_fn(|i| start[i] + segment[i] * t);
    dot(sub(point, nearest), sub(point, nearest))
}

fn segment_segment_distance_squared(p1: [f64; 3], q1: [f64; 3], p2: [f64; 3], q2: [f64; 3]) -> f64 {
    let (d1, d2, r) = (sub(q1, p1), sub(q2, p2), sub(p1, p2));
    let (a, e, f) = (dot(d1, d1), dot(d2, d2), dot(d2, r));
    let (mut s, mut t);
    if a <= 1.0e-18 && e <= 1.0e-18 {
        return dot(r, r);
    }
    if a <= 1.0e-18 {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = dot(d1, r);
        if e <= 1.0e-18 {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot(d1, d2);
            let denominator = a * e - b * b;
            s = if denominator.abs() > 1.0e-18 {
                ((b * f - c * e) / denominator).clamp(0.0, 1.0)
            } else {
                0.0
            };
            t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
        }
    }
    let delta = std::array::from_fn(|i| r[i] + d1[i] * s - d2[i] * t);
    dot(delta, delta)
}

fn segment_intersects_box(
    start: [f64; 3],
    end: [f64; 3],
    center: [f64; 3],
    half_size: [f64; 3],
    radius: f64,
) -> bool {
    let direction = sub(end, start);
    let mut lo: f64 = 0.0;
    let mut hi: f64 = 1.0;
    for axis in 0..3 {
        let minimum = center[axis] - half_size[axis] - radius;
        let maximum = center[axis] + half_size[axis] + radius;
        if direction[axis].abs() < 1.0e-12 {
            if start[axis] < minimum || start[axis] > maximum {
                return false;
            }
            continue;
        }
        let a = (minimum - start[axis]) / direction[axis];
        let b = (maximum - start[axis]) / direction[axis];
        lo = lo.max(a.min(b));
        hi = hi.min(a.max(b));
        if lo > hi {
            return false;
        }
    }
    true
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(left, right)| left * right).sum()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
