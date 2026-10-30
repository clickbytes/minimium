use std::{f64::consts::PI, fmt};

use crate::{
    environment::{EnvironmentDescriptor, EnvironmentProfile},
    world::{PlanetDefinition, Position3Mm, WorldManifest, WorldTopology},
};

pub const HEIGHT_REFERENCE_REVISION: u32 = 1;
pub const DEFAULT_OBSERVER_EYE_HEIGHT_MM: u32 = 1700;
pub const MAX_OBSERVER_POSITION_MM: u64 = 10_000_000_000;
pub const MAX_OBSERVER_EYE_HEIGHT_MM: u32 = 100_000;
const MIN_GAME_RADIUS_MM: u64 = 318_550_000;
const MAX_GAME_RADIUS_MM: u64 = 6_371_000_000;
const MIN_PHYSICAL_HEIGHT_M: f64 = -12_000.0;
const MAX_PHYSICAL_HEIGHT_M: f64 = 100_000.0;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SkyModel {
    None,
    EarthGeoreferencedV1,
    MappedEarthSkyV1,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeographicObserver {
    latitude_rad: f64,
    longitude_rad: f64,
    height_m: f64,
}

impl GeographicObserver {
    fn new(latitude_rad: f64, longitude_rad: f64, height_m: f64) -> Result<Self, ObserverError> {
        if !latitude_rad.is_finite() || !longitude_rad.is_finite() || !height_m.is_finite() {
            return Err(ObserverError::NonFinite);
        }
        if !(-PI / 2.0..=PI / 2.0).contains(&latitude_rad)
            || !(-PI..PI).contains(&longitude_rad)
            || !(MIN_PHYSICAL_HEIGHT_M..=MAX_PHYSICAL_HEIGHT_M).contains(&height_m)
        {
            return Err(ObserverError::UnsupportedHeight);
        }
        Ok(Self {
            latitude_rad,
            longitude_rad,
            height_m,
        })
    }

    pub const fn latitude_rad(self) -> f64 {
        self.latitude_rad
    }

    pub const fn longitude_rad(self) -> f64 {
        self.longitude_rad
    }

    pub const fn height_m(self) -> f64 {
        self.height_m
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EarthHeightReference {
    pub raw_gebco_m: f64,
    pub baseline_game_mm: i64,
    pub physical_anchor_m: f64,
}

impl EarthHeightReference {
    pub fn from_gebco(
        raw_gebco_m: f64,
        definition: PlanetDefinition,
    ) -> Result<Self, ObserverError> {
        if !raw_gebco_m.is_finite() {
            return Err(ObserverError::NonFinite);
        }
        if !definition.is_valid() || definition.mode != 1 {
            return Err(ObserverError::InvalidPlanet);
        }
        let scale = definition.height_scale_permille;
        if !(1..=1000).contains(&scale) {
            return Err(ObserverError::InvalidPlanet);
        }
        let scaled_gebco_mm = raw_gebco_m * f64::from(scale);
        if !scaled_gebco_mm.is_finite()
            || scaled_gebco_mm < i64::MIN as f64
            || scaled_gebco_mm > i64::MAX as f64
        {
            return Err(ObserverError::UnsupportedHeight);
        }
        let baseline_game_mm = (scaled_gebco_mm.round() as i64).clamp(
            definition.sea_level_mm,
            i64::from(definition.mountain_limit_mm),
        );
        let physical_anchor_m = raw_gebco_m.max(definition.sea_level_mm as f64 / f64::from(scale));
        if !physical_anchor_m.is_finite() {
            return Err(ObserverError::NonFinite);
        }
        Ok(Self {
            raw_gebco_m,
            baseline_game_mm,
            physical_anchor_m,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObserverFrame {
    pub model: SkyModel,
    pub geographic: GeographicObserver,
    pub north_world: [f64; 3],
    pub east_world: [f64; 3],
    pub up_world: [f64; 3],
    pub game_eye_world_m: [f64; 3],
    pub game_planet_radius_m: f64,
    pub height_reference_revision: u32,
}

impl ObserverFrame {
    /// Converts Astronomy Engine HOR coordinates `[north, west, up]` into game axes.
    pub fn hor_to_world(self, direction: [f64; 3]) -> Result<[f64; 3], ObserverError> {
        if !direction.iter().all(|component| component.is_finite()) {
            return Err(ObserverError::NonFinite);
        }
        let [north, west, up] = direction;
        let world = std::array::from_fn(|axis| {
            self.north_world[axis] * north - self.east_world[axis] * west + self.up_world[axis] * up
        });
        if world.iter().all(|component| component.is_finite()) {
            Ok(world)
        } else {
            Err(ObserverError::NonFinite)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObserverInput {
    pub feet_position_mm: Position3Mm,
    pub eye_height_mm: u32,
    pub model: SkyModel,
    pub game_radius_mm: u64,
    pub planet_definition: Option<PlanetDefinition>,
    pub height_reference: Option<EarthHeightReference>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObserverError {
    MissingPrecisePosition,
    EnvironmentProfileMismatch,
    InvalidPlanet,
    ZeroRadiusPosition,
    PositionOutOfRange,
    MissingEarthPackage,
    PackageMismatch,
    NonFinite,
    UnsupportedHeight,
    InvalidEyeHeight,
}

impl fmt::Display for ObserverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingPrecisePosition => "precise observer position is unavailable",
            Self::EnvironmentProfileMismatch => "environment profile does not match the world",
            Self::InvalidPlanet => "planet definition is invalid for sky observation",
            Self::ZeroRadiusPosition => "observer position is too close to the planet center",
            Self::PositionOutOfRange => "observer position exceeds the supported range",
            Self::MissingEarthPackage => "the matching GEBCO package is not installed",
            Self::PackageMismatch => "the installed GEBCO package differs from the planet",
            Self::NonFinite => "observer data contains a non-finite value",
            Self::UnsupportedHeight => "observer height is outside the supported range",
            Self::InvalidEyeHeight => "observer eye height is outside the supported range",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ObserverError {}

pub fn resolve_sky_model(
    manifest: WorldManifest,
    descriptor: EnvironmentDescriptor,
) -> Result<SkyModel, ObserverError> {
    match (manifest.topology(), descriptor.profile()) {
        (WorldTopology::Dungeon, EnvironmentProfile::Interior) => Ok(SkyModel::None),
        (WorldTopology::Planet, EnvironmentProfile::Earth) => {
            let definition = manifest
                .planet()
                .ok_or(ObserverError::EnvironmentProfileMismatch)?;
            if !definition.is_valid() {
                return Err(ObserverError::InvalidPlanet);
            }
            if definition.mode != 1 {
                return Err(ObserverError::EnvironmentProfileMismatch);
            }
            Ok(SkyModel::EarthGeoreferencedV1)
        }
        (WorldTopology::Planet, EnvironmentProfile::EarthLike) => {
            if manifest
                .planet()
                .is_some_and(|definition| !definition.is_valid())
            {
                return Err(ObserverError::InvalidPlanet);
            }
            if manifest
                .planet()
                .is_some_and(|definition| definition.mode == 1)
            {
                return Err(ObserverError::EnvironmentProfileMismatch);
            }
            Ok(SkyModel::MappedEarthSkyV1)
        }
        _ => Err(ObserverError::EnvironmentProfileMismatch),
    }
}

pub fn normalized_observer_direction(position: Position3Mm) -> Result<[f64; 3], ObserverError> {
    let coordinates = position.coordinates();
    if coordinates
        .iter()
        .any(|component| component.unsigned_abs() > MAX_OBSERVER_POSITION_MM)
    {
        return Err(ObserverError::PositionOutOfRange);
    }
    let vector = coordinates.map(|component| component as f64);
    let radius_mm = vector
        .iter()
        .map(|component| component * component)
        .sum::<f64>()
        .sqrt();
    if !radius_mm.is_finite() {
        return Err(ObserverError::NonFinite);
    }
    if radius_mm <= 1000.0 {
        return Err(ObserverError::ZeroRadiusPosition);
    }
    Ok(vector.map(|component| component / radius_mm))
}

pub fn try_observer_frame(input: ObserverInput) -> Result<Option<ObserverFrame>, ObserverError> {
    if input.model == SkyModel::None {
        return Ok(None);
    }
    if input.eye_height_mm > MAX_OBSERVER_EYE_HEIGHT_MM {
        return Err(ObserverError::InvalidEyeHeight);
    }
    if !(MIN_GAME_RADIUS_MM..=MAX_GAME_RADIUS_MM).contains(&input.game_radius_mm) {
        return Err(ObserverError::InvalidPlanet);
    }
    if let Some(definition) = input.planet_definition {
        if !definition.is_valid() || definition.radius_mm != input.game_radius_mm {
            return Err(ObserverError::InvalidPlanet);
        }
    }

    let direction = normalized_observer_direction(input.feet_position_mm)?;
    let position = input
        .feet_position_mm
        .coordinates()
        .map(|component| component as f64);
    let radius_position_mm = position
        .iter()
        .map(|component| component * component)
        .sum::<f64>()
        .sqrt();
    let latitude_rad = direction[1].clamp(-1.0, 1.0).asin();
    let [x_mm, _, z_mm] = input.feet_position_mm.coordinates();
    let longitude_rad = if x_mm == 0 && z_mm == 0 {
        0.0
    } else {
        let longitude = (z_mm as f64).atan2(x_mm as f64);
        if longitude >= PI {
            longitude - 2.0 * PI
        } else {
            longitude
        }
    };

    let height_m = match input.model {
        SkyModel::None => return Ok(None),
        SkyModel::EarthGeoreferencedV1 => {
            let definition = input
                .planet_definition
                .filter(|definition| definition.mode == 1)
                .ok_or(ObserverError::InvalidPlanet)?;
            if !definition.is_valid() {
                return Err(ObserverError::InvalidPlanet);
            }
            let reference = input
                .height_reference
                .ok_or(ObserverError::MissingEarthPackage)?;
            if EarthHeightReference::from_gebco(reference.raw_gebco_m, definition)? != reference {
                return Err(ObserverError::UnsupportedHeight);
            }
            let residual_eye_m = (radius_position_mm
                - input.game_radius_mm as f64
                - reference.baseline_game_mm as f64
                + f64::from(input.eye_height_mm))
                / 1000.0;
            reference.physical_anchor_m + residual_eye_m
        }
        SkyModel::MappedEarthSkyV1 => {
            if input.height_reference.is_some()
                || input
                    .planet_definition
                    .is_some_and(|definition| definition.mode == 1)
            {
                return Err(ObserverError::EnvironmentProfileMismatch);
            }
            (radius_position_mm - input.game_radius_mm as f64 + f64::from(input.eye_height_mm))
                / 1000.0
        }
    };
    let geographic = GeographicObserver::new(latitude_rad, longitude_rad, height_m)?;
    let sin_latitude = latitude_rad.sin();
    let cos_latitude = latitude_rad.cos();
    let sin_longitude = longitude_rad.sin();
    let cos_longitude = longitude_rad.cos();
    let north_world = [
        -sin_latitude * cos_longitude,
        cos_latitude,
        -sin_latitude * sin_longitude,
    ];
    let east_world = [-sin_longitude, 0.0, cos_longitude];
    let up_world = [
        cos_latitude * cos_longitude,
        sin_latitude,
        cos_latitude * sin_longitude,
    ];
    let eye_radius_m = (radius_position_mm + f64::from(input.eye_height_mm)) / 1000.0;
    let game_eye_world_m = direction.map(|component| component * eye_radius_m);
    let game_planet_radius_m = input.game_radius_mm as f64 / 1000.0;
    if !game_eye_world_m
        .iter()
        .all(|component| component.is_finite())
        || !game_planet_radius_m.is_finite()
    {
        return Err(ObserverError::NonFinite);
    }
    Ok(Some(ObserverFrame {
        model: input.model,
        geographic,
        north_world,
        east_world,
        up_world,
        game_eye_world_m,
        game_planet_radius_m,
        height_reference_revision: HEIGHT_REFERENCE_REVISION,
    }))
}
