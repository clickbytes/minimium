use minimium_world::world::{
    PlanetSectorKey, SectorKey, WorldManifest, WorldTopology, PLANET_SECTORS_PER_FACE,
};

pub const DUNGEON_SECTOR_EDGE: u16 = 16;

/// One 8-connected surface ring, including diagonals across cube-face seams.
pub fn planet_sector_neighborhood_with_diagonals(
    center: PlanetSectorKey,
    count: u16,
) -> Vec<PlanetSectorKey> {
    if count == 0 {
        return vec![center];
    }
    let cardinal = center.neighbors_with_count(count);
    let mut candidates = vec![center];
    candidates.extend(cardinal);
    for neighbor in cardinal {
        candidates.extend(neighbor.neighbors_with_count(count));
    }
    candidates.sort_unstable();
    candidates.dedup();
    let corners = sector_corners(center, count);
    candidates
        .into_iter()
        .filter(|candidate| {
            *candidate == center
                || sector_corners(*candidate, count)
                    .iter()
                    .any(|corner| corners.contains(corner))
        })
        .collect()
}

fn sector_corners(
    sector: PlanetSectorKey,
    count: u16,
) -> [minimium_world::world::CubeSampleKey; 4] {
    [
        sector.cube_sample_with_count(0, 0, 1, count),
        sector.cube_sample_with_count(1, 0, 1, count),
        sector.cube_sample_with_count(1, 1, 1, count),
        sector.cube_sample_with_count(0, 1, 1, count),
    ]
    .map(|corner| corner.expect("valid sector corner"))
}

/// Returns the server/client collision neighborhood for a topology.
pub fn sector_neighborhood(topology: WorldTopology, position: [f32; 3]) -> Vec<SectorKey> {
    neighborhood(topology, position, PLANET_SECTORS_PER_FACE)
}

pub fn sector_neighborhood_for(manifest: WorldManifest, position: [f32; 3]) -> Vec<SectorKey> {
    neighborhood(
        manifest.topology(),
        position,
        crate::planet_sector_count(manifest),
    )
}

fn neighborhood(topology: WorldTopology, position: [f32; 3], count: u16) -> Vec<SectorKey> {
    match topology {
        WorldTopology::Planet => {
            let center = PlanetSectorKey::from_direction_with_count(position, count);
            let sector_key = |sector: PlanetSectorKey| {
                let (u, v) = sector.coordinates();
                SectorKey::planet(sector.face(), u, v).expect("neighbor sector is valid")
            };
            let mut keys = Vec::with_capacity(21);
            for key in std::iter::once(center).chain(center.neighbors_with_count(count)) {
                if !keys.contains(&sector_key(key)) {
                    keys.push(sector_key(key));
                }
                for neighbor in key.neighbors_with_count(count) {
                    if !keys.contains(&sector_key(neighbor)) {
                        keys.push(sector_key(neighbor));
                    }
                }
            }
            keys
        }
        WorldTopology::Dungeon => {
            let edge = f32::from(DUNGEON_SECTOR_EDGE);
            let x = (position[0] / edge).floor() as i32;
            let y = (position[1] / edge).floor() as i32;
            let z = (position[2] / edge).floor() as i32;
            let mut keys = Vec::with_capacity(9);
            for offset_x in -1..=1 {
                for offset_z in -1..=1 {
                    keys.push(SectorKey::dungeon(x + offset_x, y, z + offset_z));
                }
            }
            keys
        }
    }
}
