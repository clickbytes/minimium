use minimium_world::world::{PlanetFace, SectorKey, WorldManifest, WorldTopology};
use std::collections::BTreeSet;

pub(crate) const MAX_COMBAT_RAY_SECTORS: usize = 4096;
pub(crate) const MAX_PENDING_COMBAT_SECTORS: usize = 8192;
const MAX_COMBAT_RAY_RANGE_M: f32 = 512.0;
const INDEX_BOUNDARY_GUARD: f64 = 1.0e-7;

/// Conservatively finds every planet sector domain touched by a finite ray.
/// Each face is a convex cone. Clipping against its five linear inequalities
/// gives an exact closed parameter interval; projected U and V are
/// fractional-linear with constant-sign derivatives there, so their endpoint
/// extrema bound every grid cell the ray can enter. The Cartesian cell range
/// may over-request, but closed bounds include tangencies and both sides of
/// cube-face, edge and corner boundaries.
pub(crate) fn combat_ray_sectors(
    manifest: WorldManifest,
    origin: [f32; 3],
    direction: [f32; 3],
    range: f32,
) -> Option<Vec<SectorKey>> {
    if !range.is_finite()
        || range <= 0.0
        || range > MAX_COMBAT_RAY_RANGE_M
        || origin
            .iter()
            .chain(direction.iter())
            .any(|value| !value.is_finite())
    {
        return None;
    }
    let length = direction
        .iter()
        .map(|value| f64::from(*value).powi(2))
        .sum::<f64>()
        .sqrt();
    if !length.is_finite() || length <= 1.0e-8 {
        return None;
    }
    if manifest.topology() != WorldTopology::Planet {
        return Some(Vec::new());
    }

    let origin = origin.map(f64::from);
    let direction = direction.map(|value| f64::from(value) / length);
    let range = f64::from(range);
    if passes_through_origin(origin, direction, range) {
        // The sector address is undefined at the planetary center; fail closed
        // instead of guessing one face for a ray crossing the singularity.
        return None;
    }

    let count = minimium_procgen::planet_sector_count(manifest);
    let mut keys = BTreeSet::new();
    for face in PlanetFace::ALL {
        let (normal, u_axis, v_axis) = face_basis(face);
        let mut interval = (0.0, range);
        let constraints = [
            normal,
            add(normal, u_axis),
            subtract(normal, u_axis),
            add(normal, v_axis),
            subtract(normal, v_axis),
        ];
        if constraints
            .into_iter()
            .any(|constraint| !clip_nonnegative(constraint, origin, direction, range, &mut interval))
        {
            continue;
        }
        if interval.0 > interval.1 {
            continue;
        }
        let u = projected_range(u_axis, normal, origin, direction, interval)?;
        let v = projected_range(v_axis, normal, origin, direction, interval)?;
        let u_range = sector_index_range(u, count)?;
        let v_range = sector_index_range(v, count)?;
        for u in u_range.0..=u_range.1 {
            for v in v_range.0..=v_range.1 {
                keys.insert(SectorKey::planet(face, u, v)?);
                if keys.len() > MAX_COMBAT_RAY_SECTORS {
                    return None;
                }
            }
        }
    }
    Some(keys.into_iter().collect())
}

fn clip_nonnegative(
    axis: [f64; 3],
    origin: [f64; 3],
    direction: [f64; 3],
    range: f64,
    interval: &mut (f64, f64),
) -> bool {
    let start = dot(axis, origin);
    let slope = dot(axis, direction);
    let tolerance = 1.0e-12 * start.abs().max((slope * range).abs()).max(1.0);
    if slope.abs() <= f64::EPSILON {
        return start >= -tolerance;
    }
    let crossing = (-tolerance - start) / slope;
    if slope > 0.0 {
        interval.0 = interval.0.max(crossing);
    } else {
        interval.1 = interval.1.min(crossing);
    }
    interval.0 <= interval.1
}

fn projected_range(
    axis: [f64; 3],
    normal: [f64; 3],
    origin: [f64; 3],
    direction: [f64; 3],
    interval: (f64, f64),
) -> Option<(f64, f64)> {
    let ratio = |distance: f64| {
        let point = std::array::from_fn(|index| origin[index] + direction[index] * distance);
        let denominator = dot(normal, point);
        if denominator.abs() <= 1.0e-10 {
            let direction_denominator = dot(normal, direction);
            (direction_denominator.abs() > 1.0e-14)
                .then(|| dot(axis, direction) / direction_denominator)
        } else {
            Some(dot(axis, point) / denominator)
        }
    };
    let left = ratio(interval.0)?;
    let right = ratio(interval.1)?;
    Some((left.min(right).clamp(-1.0, 1.0), left.max(right).clamp(-1.0, 1.0)))
}

fn sector_index_range(projected: (f64, f64), count: u16) -> Option<(u16, u16)> {
    if count == 0 {
        return None;
    }
    let count = f64::from(count);
    let grid_min = ((projected.0 + 1.0) * 0.5 * count - INDEX_BOUNDARY_GUARD).floor();
    let grid_max = ((projected.1 + 1.0) * 0.5 * count + INDEX_BOUNDARY_GUARD).floor();
    let last = count as u16 - 1;
    let first = (grid_min as i64).clamp(0, i64::from(last)) as u16;
    let final_index = (grid_max as i64).clamp(0, i64::from(last)) as u16;
    (first <= final_index).then_some((first, final_index))
}

fn passes_through_origin(
    origin: [f64; 3],
    direction: [f64; 3],
    range: f64,
) -> bool {
    let closest = (-dot(origin, direction)).clamp(0.0, range);
    let distance = std::array::from_fn::<_, 3, _>(|axis| origin[axis] + direction[axis] * closest);
    dot(distance, distance) <= 1.0e-12
}

fn face_basis(face: PlanetFace) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let as_f64 = |axis: [i8; 3]| axis.map(f64::from);
    match face {
        PlanetFace::PositiveX => (as_f64([1, 0, 0]), as_f64([0, 1, 0]), as_f64([0, 0, 1])),
        PlanetFace::NegativeX => (as_f64([-1, 0, 0]), as_f64([0, 1, 0]), as_f64([0, 0, 1])),
        PlanetFace::PositiveY => (as_f64([0, 1, 0]), as_f64([1, 0, 0]), as_f64([0, 0, 1])),
        PlanetFace::NegativeY => (as_f64([0, -1, 0]), as_f64([1, 0, 0]), as_f64([0, 0, 1])),
        PlanetFace::PositiveZ => (as_f64([0, 0, 1]), as_f64([1, 0, 0]), as_f64([0, 1, 0])),
        PlanetFace::NegativeZ => (as_f64([0, 0, -1]), as_f64([1, 0, 0]), as_f64([0, 1, 0])),
    }
}

fn add(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| left[axis] + right[axis])
}

fn subtract(left: [f64; 3], right: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| left[axis] - right[axis])
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    (0..3).map(|axis| left[axis] * right[axis]).sum()
}
