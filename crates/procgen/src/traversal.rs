use minimium_core::world::{PlanetKind, Position3Mm, WorldManifest, WorldTopology};

/// Applies the shared surface movement policy for a world manifest.
///
/// Worlds without an explicit Earth definition keep their existing movement
/// rules. Millimetre positions are converted to metres only at the adapter
/// boundary required by the existing Earth elevation function.
pub fn can_traverse_surface(manifest: WorldManifest, position_mm: Position3Mm) -> bool {
    let Some(planet) = manifest.planet() else {
        return true;
    };
    if manifest.topology() != WorldTopology::Planet || planet.kind() != PlanetKind::Earth {
        return true;
    }
    let position_m = position_mm.0.map(|value| (value as f64 / 1000.0) as f32);
    crate::earth_is_land(
        position_m,
        planet.height_scale_permille,
        planet.sea_level_mm,
    )
}
