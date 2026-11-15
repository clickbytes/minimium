use super::*;

pub(super) fn entity_sector_for_position(
    manifest: WorldManifest,
    position: [f32; 3],
) -> Option<SectorKey> {
    if position.iter().any(|coordinate| !coordinate.is_finite()) {
        return None;
    }
    match manifest.topology() {
        WorldTopology::Planet => {
            if position.iter().all(|coordinate| *coordinate == 0.0) {
                return None;
            }
            let sector = minimium_world::world::PlanetSectorKey::from_direction_with_count(
                position,
                minimium_procgen::planet_sector_count(manifest),
            );
            let (u, v) = sector.coordinates();
            SectorKey::planet(sector.face(), u, v)
        }
        WorldTopology::Dungeon => {
            let edge = f32::from(minimium_procgen::DUNGEON_SECTOR_EDGE);
            let coordinate = |value: f32| {
                let value = (value / edge).floor();
                (value >= i32::MIN as f32 && value <= i32::MAX as f32).then_some(value as i32)
            };
            Some(SectorKey::dungeon(
                coordinate(position[0])?,
                coordinate(position[1])?,
                coordinate(position[2])?,
            ))
        }
    }
}
