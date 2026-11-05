use minimium_world::world::{PlanetKind, Position3Mm, WorldManifest, WorldTopology};

/// Applies the shared surface movement policy for a world manifest.
///
/// Worlds without an explicit Earth definition keep their existing movement
/// rules. Millimetre positions are converted to metres only at the adapter
/// boundary required by the existing Earth elevation function.
pub fn can_traverse_surface(
    manifest: WorldManifest,
    position_mm: Position3Mm,
    geography: Option<&dyn crate::PreparedGeography>,
) -> bool {
    let Some(planet) = manifest.planet() else {
        return true;
    };
    if manifest.topology() != WorldTopology::Planet || planet.kind() != PlanetKind::Earth {
        return true;
    }
    let position_m = position_mm.0.map(|value| (value as f64 / 1000.0) as f32);
    geography
        .and_then(|geography| geography.elevation_m(position_m.map(f64::from)).ok())
        .is_some_and(|elevation_m| {
            (elevation_m * f64::from(planet.height_scale_permille)).round() as i64
                >= planet.sea_level_mm
        })
}
