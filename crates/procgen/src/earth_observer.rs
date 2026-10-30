use minimium_core::{
    sky::{normalized_observer_direction, EarthHeightReference, ObserverError},
    world::{PlanetDefinition, Position3Mm},
};

use crate::{earth::EarthSampleError, try_earth_elevation_m_for_package};

pub fn earth_height_reference(
    position_mm: Position3Mm,
    definition: PlanetDefinition,
) -> Result<EarthHeightReference, ObserverError> {
    if !definition.is_valid() || definition.mode != 1 {
        return Err(ObserverError::InvalidPlanet);
    }
    let direction = normalized_observer_direction(position_mm)?;
    let elevation_m = try_earth_elevation_m_for_package(direction, definition.package_digest)
        .map_err(map_sample_error)?;
    EarthHeightReference::from_gebco(elevation_m, definition)
}

fn map_sample_error(error: EarthSampleError) -> ObserverError {
    match error {
        EarthSampleError::PackageUnavailable => ObserverError::MissingEarthPackage,
        EarthSampleError::PackageMismatch => ObserverError::PackageMismatch,
        EarthSampleError::InvalidDirection => ObserverError::NonFinite,
    }
}
