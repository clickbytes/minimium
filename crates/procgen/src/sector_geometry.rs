use crate::sector_neighborhood::DUNGEON_SECTOR_EDGE;
use minimium_core::world::{
    MapBox, MapTriangle, PlanetFace, PlanetSectorKey, SectorKey, WorldMap, WorldTopology,
    PLANET_SECTORS_PER_FACE,
};

#[derive(Clone, Default)]
pub struct SectorGeometry {
    pub boxes: Vec<MapBox>,
    pub triangles: Vec<MapTriangle>,
}

/// Slices render and collision geometry using the shared planet or dungeon
/// sector address contract.
pub fn slice_sector_geometry(
    map: &WorldMap,
    topology: WorldTopology,
    key: SectorKey,
) -> SectorGeometry {
    let mut geometry = SectorGeometry::default();
    for map_box in map.boxes() {
        let included = match (topology, key) {
            (WorldTopology::Planet, SectorKey::Planet(sector)) => {
                planet_box_touches(*map_box, sector)
            }
            (WorldTopology::Dungeon, SectorKey::Dungeon(sector)) => {
                dungeon_box_overlaps(*map_box, sector.coordinates())
            }
            _ => false,
        };
        if included {
            geometry.boxes.push(*map_box);
        }
    }
    for triangle in map.triangles() {
        let included = match (topology, key) {
            (WorldTopology::Planet, SectorKey::Planet(sector)) => {
                planet_triangle_touches(*triangle, sector)
            }
            (WorldTopology::Dungeon, SectorKey::Dungeon(sector)) => {
                dungeon_triangle_touches(*triangle, sector.coordinates())
            }
            _ => false,
        };
        if included {
            geometry.triangles.push(*triangle);
        }
    }
    geometry
}

fn planet_box_touches(map_box: MapBox, sector: PlanetSectorKey) -> bool {
    let half = map_box.size.map(|value| value.abs() * 0.5);
    let mut directions = Vec::with_capacity(9);
    directions.push(map_box.center);
    for x in [-half[0], half[0]] {
        for y in [-half[1], half[1]] {
            for z in [-half[2], half[2]] {
                directions.push([
                    map_box.center[0] + x,
                    map_box.center[1] + y,
                    map_box.center[2] + z,
                ]);
            }
        }
    }
    directions.into_iter().any(|direction| {
        let sample = PlanetSectorKey::from_direction(direction);
        sample == sector || sample.neighbors().contains(&sector)
    })
}

fn planet_triangle_touches(triangle: MapTriangle, sector: PlanetSectorKey) -> bool {
    let centroid = std::array::from_fn(|axis| {
        triangle
            .vertices
            .iter()
            .map(|point| point[axis])
            .sum::<f32>()
            / 3.0
    });
    let face = PlanetSectorKey::from_direction(centroid).face();
    if sector.face() != face {
        return false;
    }

    let (normal, u_axis, v_axis) = planet_face_basis(face);
    let component = |point: [f32; 3], axis: [i8; 3]| {
        point[0] * f32::from(axis[0])
            + point[1] * f32::from(axis[1])
            + point[2] * f32::from(axis[2])
    };
    let mut minimum = [f32::INFINITY; 2];
    let mut maximum = [f32::NEG_INFINITY; 2];
    for point in triangle.vertices {
        let denominator = component(point, normal);
        if denominator <= f32::EPSILON {
            return false;
        }
        for (axis, basis) in [u_axis, v_axis].into_iter().enumerate() {
            let coordinate = component(point, basis) / denominator;
            minimum[axis] = minimum[axis].min(coordinate);
            maximum[axis] = maximum[axis].max(coordinate);
        }
    }
    let (u, v) = sector.coordinates();
    overlaps_planet_interval(minimum[0], maximum[0], u)
        && overlaps_planet_interval(minimum[1], maximum[1], v)
}

fn planet_face_basis(face: PlanetFace) -> ([i8; 3], [i8; 3], [i8; 3]) {
    match face {
        PlanetFace::PositiveX => ([1, 0, 0], [0, 1, 0], [0, 0, 1]),
        PlanetFace::NegativeX => ([-1, 0, 0], [0, 1, 0], [0, 0, 1]),
        PlanetFace::PositiveY => ([0, 1, 0], [1, 0, 0], [0, 0, 1]),
        PlanetFace::NegativeY => ([0, -1, 0], [1, 0, 0], [0, 0, 1]),
        PlanetFace::PositiveZ => ([0, 0, 1], [1, 0, 0], [0, 1, 0]),
        PlanetFace::NegativeZ => ([0, 0, -1], [1, 0, 0], [0, 1, 0]),
    }
}

fn overlaps_planet_interval(minimum: f32, maximum: f32, sector: u16) -> bool {
    let count = f32::from(PLANET_SECTORS_PER_FACE);
    let start = -1.0 + 2.0 * f32::from(sector) / count;
    let end = start + 2.0 / count;
    minimum <= end && maximum >= start
}

fn dungeon_box_overlaps(map_box: MapBox, (sector_x, sector_y, sector_z): (i32, i32, i32)) -> bool {
    let half = map_box.size.map(|value| value * 0.5);
    let center = map_box.center;
    overlaps_interval(center[0] - half[0], center[0] + half[0], sector_x)
        && overlaps_interval(center[1] - half[1], center[1] + half[1], sector_y)
        && overlaps_interval(center[2] - half[2], center[2] + half[2], sector_z)
}

fn dungeon_triangle_touches(
    triangle: MapTriangle,
    (sector_x, sector_y, sector_z): (i32, i32, i32),
) -> bool {
    let minimum = std::array::from_fn::<_, 3, _>(|axis| {
        triangle
            .vertices
            .iter()
            .map(|point| point[axis])
            .fold(f32::INFINITY, f32::min)
    });
    let maximum = std::array::from_fn::<_, 3, _>(|axis| {
        triangle
            .vertices
            .iter()
            .map(|point| point[axis])
            .fold(f32::NEG_INFINITY, f32::max)
    });
    overlaps_interval(minimum[0], maximum[0], sector_x)
        && overlaps_interval(minimum[1], maximum[1], sector_y)
        && overlaps_interval(minimum[2], maximum[2], sector_z)
}

fn overlaps_interval(minimum: f32, maximum: f32, sector: i32) -> bool {
    let edge = f32::from(DUNGEON_SECTOR_EDGE);
    let start = sector as f32 * edge;
    minimum <= start + edge && maximum >= start
}
