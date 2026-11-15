//! Bounded presentation-only sky visibility for excavation surfaces.
const SKY_RAY_COUNT: usize = 5;
const SKY_RAY_STEPS: usize = 24;
const SKY_RAY_DISTANCE_M: f64 = 8.0;
const SURFACE_EPSILON_M: f64 = 0.025;

pub(crate) fn sky_visibility(
    mut sample_distance: impl FnMut([f64; 3]) -> Option<f64>,
    point_m: [f64; 3],
    normal: [f32; 3],
) -> f32 {
    let radial_length = dot(point_m, point_m).sqrt();
    if radial_length <= f64::EPSILON {
        return 0.0;
    }
    let radial = point_m.map(|value| value / radial_length);
    let surface_normal = normalize(normal.map(f64::from));
    let hemisphere = if dot(surface_normal, surface_normal) > f64::EPSILON {
        surface_normal
    } else {
        radial
    };
    let surface_normal = hemisphere;
    let axis = if hemisphere[0].abs() <= hemisphere[1].abs()
        && hemisphere[0].abs() <= hemisphere[2].abs()
    {
        [1.0, 0.0, 0.0]
    } else if hemisphere[1].abs() <= hemisphere[2].abs() {
        [0.0, 1.0, 0.0]
    } else {
        [0.0, 0.0, 1.0]
    };
    let tangent = normalize(cross(hemisphere, axis));
    let bitangent = normalize(cross(hemisphere, tangent));
    let mut directions = [[0.0; 3]; SKY_RAY_COUNT];
    directions[0] = hemisphere;
    directions[1] = normalize(add(hemisphere, scale(tangent, 0.45)));
    directions[2] = normalize(add(hemisphere, scale(tangent, -0.45)));
    directions[3] = normalize(add(hemisphere, scale(bitangent, 0.45)));
    directions[4] = normalize(add(hemisphere, scale(bitangent, -0.45)));
    let visible = directions
        .into_iter()
        .filter(|direction| ray_visible(&mut sample_distance, point_m, surface_normal, *direction))
        .count();
    visible as f32 / SKY_RAY_COUNT as f32
}

fn ray_visible(
    sample_distance: &mut impl FnMut([f64; 3]) -> Option<f64>,
    point: [f64; 3],
    normal: [f64; 3],
    direction: [f64; 3],
) -> bool {
    let mut distance_m = 0.0;
    let start = add(point, scale(normal, SURFACE_EPSILON_M));
    for _ in 0..SKY_RAY_STEPS {
        let sample_point = add(start, scale(direction, distance_m));
        let Some(distance) = sample_distance(sample_point) else {
            return false;
        };
        if distance <= SURFACE_EPSILON_M {
            return false;
        }
        let remaining = SKY_RAY_DISTANCE_M - distance_m;
        if distance >= remaining {
            return true;
        }
        distance_m += distance.min(1.0).max(0.05);
        if distance_m >= SKY_RAY_DISTANCE_M {
            return true;
        }
    }
    // Budget exhaustion is dark so a narrow roof cannot leak full ambient.
    false
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|axis| a[axis] + b[axis])
}

fn scale(vector: [f64; 3], amount: f64) -> [f64; 3] {
    vector.map(|value| value * amount)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|axis| a[axis] * b[axis]).sum()
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize(vector: [f64; 3]) -> [f64; 3] {
    let length = dot(vector, vector).sqrt();
    if length > f64::EPSILON {
        vector.map(|value| value / length)
    } else {
        [0.0, 1.0, 0.0]
    }
}
