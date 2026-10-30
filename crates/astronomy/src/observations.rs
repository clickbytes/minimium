use crate::SkyState;
use minimium_core::universe::{BodyKind, CelestialBodyId, SolarSystemId, UniverseCatalog};

pub const SKY_SUN_ID: CelestialBodyId =
    CelestialBodyId::new(1).expect("reserved sky Sun ID is nonzero");
pub const SKY_MOON_ID: CelestialBodyId =
    CelestialBodyId::new(2).expect("reserved sky Moon ID is nonzero");

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObservedBody {
    pub id: CelestialBodyId,
    pub kind: BodyKind,
    pub direction_world: [f64; 3],
    pub distance_m: f64,
    pub angular_radius_rad: f64,
    pub phase: f64,
    pub emitted_linear_rgb: [f64; 3],
    pub reflected_linear_rgb: [f64; 3],
    pub scattered_linear_rgb: [f64; 3],
    pub visible: bool,
}

/// Converts the authoritative Sun/Moon state and merges optional catalog bodies.
/// Sky bodies have reserved IDs 1 and 2; their radiometry remains owned by the
/// existing Earth lighting path. Catalog directions are rotated to world axes
/// once through the epoch-to-world matrix supplied by SkyState.
pub fn observed_bodies(
    sky: &SkyState,
    catalog: Option<(&UniverseCatalog, SolarSystemId, [f64; 3], f64)>,
) -> Vec<ObservedBody> {
    let mut bodies = vec![
        ObservedBody {
            id: SKY_SUN_ID,
            kind: BodyKind::Star,
            direction_world: sky.sun.geometric_direction_world,
            distance_m: sky.sun.distance_m,
            angular_radius_rad: sky.sun.angular_radius_rad,
            phase: 1.0,
            emitted_linear_rgb: [0.0; 3],
            reflected_linear_rgb: [0.0; 3],
            scattered_linear_rgb: [0.0; 3],
            visible: true,
        },
        ObservedBody {
            id: SKY_MOON_ID,
            kind: BodyKind::Moon,
            direction_world: sky.moon.body.geometric_direction_world,
            distance_m: sky.moon.body.distance_m,
            angular_radius_rad: sky.moon.body.angular_radius_rad,
            phase: sky.moon.illuminated_fraction,
            emitted_linear_rgb: [0.0; 3],
            reflected_linear_rgb: [0.0; 3],
            scattered_linear_rgb: [0.0; 3],
            visible: true,
        },
    ];
    if let Some((catalog, system, observer_m, time_seconds)) = catalog {
        if let Ok(observations) = catalog.observe(system, observer_m, time_seconds) {
            bodies.extend(observations.into_iter().filter_map(|observation| {
                if observation.id == SKY_SUN_ID || observation.id == SKY_MOON_ID {
                    return None;
                }
                let direction_world =
                    rotate_direction(sky.catalog_epoch_to_world, observation.direction);
                (direction_world.iter().all(|value| value.is_finite())
                    && observation.distance_m.is_finite()
                    && observation.angular_radius_rad.is_finite()
                    && observation.illuminated_fraction.is_finite()
                    && observation
                        .emitted_linear_rgb
                        .iter()
                        .all(|value| value.is_finite())
                    && observation
                        .reflected_linear_rgb
                        .iter()
                        .all(|value| value.is_finite())
                    && observation
                        .atmosphere_limb_rgb
                        .iter()
                        .all(|value| value.is_finite()))
                .then_some(ObservedBody {
                    id: observation.id,
                    kind: observation.kind,
                    direction_world,
                    distance_m: observation.distance_m,
                    angular_radius_rad: observation.angular_radius_rad,
                    phase: observation.illuminated_fraction,
                    emitted_linear_rgb: observation.emitted_linear_rgb,
                    reflected_linear_rgb: observation.reflected_linear_rgb,
                    scattered_linear_rgb: observation.atmosphere_limb_rgb,
                    visible: observation.visible,
                })
            }));
        }
    }
    bodies.sort_unstable_by_key(|body| body.id.get());
    bodies
}

fn rotate_direction(matrix: [[f64; 3]; 3], direction: [f64; 3]) -> [f64; 3] {
    let rotated = std::array::from_fn(|row| {
        matrix[row][0] * direction[0]
            + matrix[row][1] * direction[1]
            + matrix[row][2] * direction[2]
    });
    let length = rotated
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if length.is_finite() && length > 0.0 {
        rotated.map(|value| value / length)
    } else {
        [0.0; 3]
    }
}
