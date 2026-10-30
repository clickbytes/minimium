use std::f64::consts::{FRAC_PI_2, TAU};

use minimium_core::sky::{ObserverFrame, SkyModel};

use crate::{
    ffi,
    model::{CelestialBodyState, RefractionMode, SkyError},
};

pub(crate) type Matrix3 = [[f64; 3]; 3];

pub(crate) fn validate_observer(
    observer: &ObserverFrame,
    expected_revision: u32,
) -> Result<(), SkyError> {
    let geographic = observer.geographic;
    let latitude = geographic.latitude_rad();
    let longitude = geographic.longitude_rad();
    let height = geographic.height_m();
    if observer.model == SkyModel::None
        || observer.height_reference_revision != expected_revision
        || !latitude.is_finite()
        || !longitude.is_finite()
        || !height.is_finite()
        || !(-FRAC_PI_2..=FRAC_PI_2).contains(&latitude)
        || !(-std::f64::consts::PI..std::f64::consts::PI).contains(&longitude)
        || !(-12_000.0..=100_000.0).contains(&height)
        || !observer.game_planet_radius_m.is_finite()
        || observer.game_planet_radius_m <= 0.0
        || !observer
            .game_eye_world_m
            .iter()
            .all(|value| value.is_finite())
    {
        return Err(SkyError::InvalidObserver);
    }
    validate_basis(observer)
}

fn validate_basis(observer: &ObserverFrame) -> Result<(), SkyError> {
    let north = validate_vector(observer.north_world)?;
    let east = validate_vector(observer.east_world)?;
    let up = validate_vector(observer.up_world)?;
    if (length(north) - 1.0).abs() > 1.0e-8
        || (length(east) - 1.0).abs() > 1.0e-8
        || (length(up) - 1.0).abs() > 1.0e-8
        || dot(north, east).abs() > 1.0e-8
        || dot(north, up).abs() > 1.0e-8
        || dot(east, up).abs() > 1.0e-8
        || dot(cross(north, east), up) < 0.999_999_99
    {
        return Err(SkyError::InvalidObserver);
    }
    Ok(())
}

pub(crate) fn validate_vector(vector: [f64; 3]) -> Result<[f64; 3], SkyError> {
    if !vector.iter().all(|value| value.is_finite()) {
        return Err(SkyError::NonFinite);
    }
    if length(vector) <= f64::MIN_POSITIVE {
        return Err(SkyError::DegenerateVector);
    }
    Ok(vector)
}

pub(crate) fn normalize(vector: [f64; 3]) -> Result<[f64; 3], SkyError> {
    let vector = validate_vector(vector)?;
    let magnitude = length(vector);
    if !magnitude.is_finite() || magnitude <= f64::MIN_POSITIVE {
        return Err(SkyError::DegenerateVector);
    }
    Ok(vector.map(|value| value / magnitude))
}

pub(crate) fn length(vector: [f64; 3]) -> f64 {
    vector[0].hypot(vector[1]).hypot(vector[2])
}

pub(crate) fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub(crate) fn matrix_vector(matrix: Matrix3, vector: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|row| dot(matrix[row], vector))
}

pub(crate) fn matrix_multiply(a: Matrix3, b: Matrix3) -> Matrix3 {
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            a[row][0] * b[0][column] + a[row][1] * b[1][column] + a[row][2] * b[2][column]
        })
    })
}

pub(crate) fn hor_to_world_matrix(observer: &ObserverFrame) -> Matrix3 {
    let north = observer.north_world;
    let west = observer.east_world.map(|value| -value);
    let up = observer.up_world;
    [
        [north[0], west[0], up[0]],
        [north[1], west[1], up[1]],
        [north[2], west[2], up[2]],
    ]
}

pub(crate) fn validate_rotation(matrix: Matrix3) -> Result<Matrix3, SkyError> {
    if !matrix.iter().flatten().all(|value| value.is_finite()) {
        return Err(SkyError::NonFinite);
    }
    for row in matrix {
        if row.iter().any(|value| value.abs() > 1.000_001) || (length(row) - 1.0).abs() > 1.0e-6 {
            return Err(SkyError::InvalidUpstreamValue);
        }
    }
    if dot(matrix[0], matrix[1]).abs() > 1.0e-6
        || dot(matrix[0], matrix[2]).abs() > 1.0e-6
        || dot(matrix[1], matrix[2]).abs() > 1.0e-6
        || dot(cross(matrix[0], matrix[1]), matrix[2]) < 0.999_99
    {
        return Err(SkyError::InvalidUpstreamValue);
    }
    Ok(matrix)
}

pub(crate) fn body_state(
    eqj_direction: [f64; 3],
    distance_m: f64,
    radius_m: f64,
    eqj_to_hor: Matrix3,
    eqj_to_world: Matrix3,
    observer: &ObserverFrame,
    refraction: RefractionMode,
) -> Result<CelestialBodyState, SkyError> {
    let eqj_direction = normalize(eqj_direction)?;
    if !distance_m.is_finite() || distance_m <= radius_m || radius_m <= 0.0 {
        return Err(SkyError::DegenerateVector);
    }
    let geometric_direction_world = normalize(matrix_vector(eqj_to_world, eqj_direction))?;
    let geometric_hor = normalize(matrix_vector(eqj_to_hor, eqj_direction))?;
    let geometric_altitude_rad = geometric_hor[2].clamp(-1.0, 1.0).asin();
    let horizontal_length = geometric_hor[0].hypot(geometric_hor[1]);
    let azimuth_rad = if horizontal_length <= 1.0e-12 {
        0.0
    } else {
        (-geometric_hor[1]).atan2(geometric_hor[0]).rem_euclid(TAU)
    };
    let apparent_altitude_rad = match refraction {
        RefractionMode::None => geometric_altitude_rad,
        RefractionMode::Standard => {
            let lift = ffi::refraction(geometric_altitude_rad.to_degrees())
                .map_err(crate::ephemeris_cache::map_status)?;
            if !lift.is_finite() || !(-5.0..=5.0).contains(&lift) {
                return Err(SkyError::InvalidUpstreamValue);
            }
            (geometric_altitude_rad + lift.to_radians()).clamp(-FRAC_PI_2, FRAC_PI_2)
        }
    };
    let cos_altitude = apparent_altitude_rad.cos();
    let apparent_hor = [
        cos_altitude * azimuth_rad.cos(),
        -cos_altitude * azimuth_rad.sin(),
        apparent_altitude_rad.sin(),
    ];
    let apparent_direction_world =
        normalize(matrix_vector(hor_to_world_matrix(observer), apparent_hor))?;
    let angular_radius_rad = (radius_m / distance_m).clamp(-1.0, 1.0).asin();
    if !angular_radius_rad.is_finite() {
        return Err(SkyError::NonFinite);
    }
    Ok(CelestialBodyState {
        geometric_direction_world,
        apparent_direction_world,
        geometric_altitude_rad,
        apparent_altitude_rad,
        azimuth_rad,
        distance_m,
        angular_radius_rad,
    })
}
