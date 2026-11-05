use minimium_environment::sky::{
    normalized_observer_direction, EarthHeightReference, ObserverError,
};
use minimium_world::world::{PlanetDefinition, Position3Mm};

use crate::{EarthSampleError, PreparedGeography};

pub fn earth_height_reference(
    position_mm: Position3Mm,
    definition: PlanetDefinition,
    geography: Option<&dyn PreparedGeography>,
) -> Result<EarthHeightReference, ObserverError> {
    if !definition.is_valid() || definition.mode != 1 {
        return Err(ObserverError::InvalidPlanet);
    }
    let direction = normalized_observer_direction(position_mm)?;
    let geography = geography.ok_or(ObserverError::MissingEarthPackage)?;
    if geography.package_digest() != definition.package_digest {
        return Err(ObserverError::PackageMismatch);
    }
    let elevation_m = geography.elevation_m(direction).map_err(map_sample_error)?;
    EarthHeightReference::from_gebco(elevation_m, definition)
}

fn map_sample_error(error: EarthSampleError) -> ObserverError {
    match error {
        EarthSampleError::PackageUnavailable => ObserverError::MissingEarthPackage,
        EarthSampleError::PackageMismatch => ObserverError::PackageMismatch,
        EarthSampleError::InvalidDirection => ObserverError::NonFinite,
    }
}
