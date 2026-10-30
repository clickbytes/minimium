use crate::{generate_planet_sector, planet_sector_count};
use minimium_core::world::{PlanetSectorKey, WorldManifest};

/// Intersect a globe click with the same triangles used for collision.
/// The sampled elevation alone can sit below a triangle on steep slopes.
pub fn planet_collision_position_at_direction(
    manifest: WorldManifest,
    direction: [f32; 3],
) -> Option<[i64; 3]> {
    let key = PlanetSectorKey::from_direction_with_count(direction, planet_sector_count(manifest));
    let geometry = generate_planet_sector(manifest, key);
    let d = direction.map(f64::from);
    for triangle in &geometry.triangles {
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
            return Some(d.map(|value| (value * distance).round() as i64));
        }
    }
    None
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
