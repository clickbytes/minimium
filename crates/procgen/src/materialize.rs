use crate::terrain_color::{mostly_ocean, terrain_color};
use crate::{DungeonBlueprint, DungeonTile, GenerationError, PlanetLandmarks, PlanetTerrain};
use minimium_world::world::{
    MapBox, MapTriangle, PlanetFace, WorldMap, PLANET_SAMPLE_COORDINATE_SCALE,
};

const ROOM_COLOR: [f32; 3] = [0.34, 0.32, 0.28];
const WALL_COLOR: [f32; 3] = [0.28, 0.30, 0.33];
const PLANET_SPAWN_CLEARANCE_MM: i64 = 2_000;

/// Bounded visual overview of all six faces.
pub fn materialize_planet_overview(terrain: PlanetTerrain) -> Vec<MapTriangle> {
    let steps: usize = if terrain.is_earth() { 96 } else { 24 };
    let scale = PLANET_SAMPLE_COORDINATE_SCALE;
    let mut triangles = Vec::with_capacity(6 * steps * steps * 2);
    for face in PlanetFace::ALL {
        let mut points = Vec::with_capacity((steps + 1) * (steps + 1));
        let mut ocean = Vec::with_capacity((steps + 1) * (steps + 1));
        let mut heights = Vec::with_capacity((steps + 1) * (steps + 1));
        for v in 0..=steps {
            for u in 0..=steps {
                let sample = face
                    .cube_sample(
                        -scale + 2 * scale * u as i32 / steps as i32,
                        -scale + 2 * scale * v as i32 / steps as i32,
                    )
                    .expect("overview samples lie on a cube face");
                let point = terrain.sample(sample);
                points.push(millimeters_to_world(point.position_mm.coordinates()));
                ocean.push(point.ocean);
                heights.push(
                    point.height_mm as i64 - terrain.radius_mm() as i64 - terrain.sea_level_mm(),
                );
            }
        }
        for v in 0..steps {
            for u in 0..steps {
                let a = v * (steps + 1) + u;
                let b = (v + 1) * (steps + 1) + u;
                let c = a + 1;
                let d = b + 1;
                for indices in [[a, b, c], [c, b, d]] {
                    let mut vertices = indices.map(|index| points[index]);
                    let water = mostly_ocean(indices.map(|index| ocean[index]));
                    let average_height =
                        indices.map(|index| heights[index]).into_iter().sum::<i64>() / 3;
                    let color = terrain_color(average_height, water);
                    let a = vertices[0];
                    let b = vertices[1];
                    let c = vertices[2];
                    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                    let normal = [
                        ab[1] * ac[2] - ab[2] * ac[1],
                        ab[2] * ac[0] - ab[0] * ac[2],
                        ab[0] * ac[1] - ab[1] * ac[0],
                    ];
                    if normal[0] * a[0] + normal[1] * a[1] + normal[2] * a[2] < 0.0 {
                        vertices.swap(1, 2);
                    }
                    triangles.push(MapTriangle {
                        vertices,
                        precise_vertices: None,
                        color,
                        solid: false,
                    });
                }
            }
        }
    }
    triangles
}

/// Produces the landing marker. Terrain is generated per sector on demand.
pub fn materialize_planet_map(
    terrain: PlanetTerrain,
    landmarks: PlanetLandmarks,
) -> Result<WorldMap, GenerationError> {
    let _ = terrain;
    let spawn_mm = radial_clearance(
        landmarks.spawn.position_mm.coordinates(),
        PLANET_SPAWN_CLEARANCE_MM,
    );
    let spawn = millimeters_to_world(spawn_mm);
    let entrance = millimeters_to_world(landmarks.entrance.position_mm.coordinates());
    let marker = MapBox {
        center: [entrance[0], entrance[1] + 0.6, entrance[2]],
        size: [0.65, 1.2, 0.65],
        color: [0.86, 0.53, 0.16],
        solid: false,
    };
    WorldMap::from_parts(spawn, vec![marker], Vec::new())
        .map_err(GenerationError::materialization)
        .map(|map| map.with_spawn_position_mm(minimium_world::world::Position3Mm(spawn_mm)))
}

/// Turns a connected dungeon floor plan into shared render and collision geometry.
pub fn materialize_dungeon_map(blueprint: &DungeonBlueprint) -> Result<WorldMap, GenerationError> {
    let (width, height) = blueprint.dimensions();
    let mut triangles = Vec::new();
    let mut walls = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let center = [f32::from(x) + 0.5, 0.0, f32::from(y) + 0.5];
            match blueprint.tile(x, y) {
                Some(DungeonTile::Room | DungeonTile::Corridor) => {
                    let color = if blueprint.tile(x, y) == Some(DungeonTile::Room) {
                        ROOM_COLOR
                    } else {
                        [0.27, 0.29, 0.25]
                    };
                    triangles.extend(floor_triangles(x, y, color));
                }
                Some(DungeonTile::Void) => walls.push(MapBox {
                    center: [center[0], 1.5, center[2]],
                    size: [1.0, 3.0, 1.0],
                    color: WALL_COLOR,
                    solid: true,
                }),
                None => {
                    return Err(GenerationError::invalid_blueprint(
                        "dungeon blueprint dimensions are inconsistent",
                    ))
                }
            }
        }
    }
    let spawn_cell = blueprint.spawn();
    let spawn = [
        f32::from(spawn_cell[0]) + 0.5,
        0.0,
        f32::from(spawn_cell[1]) + 0.5,
    ];
    let entrance_cell = blueprint.entrance();
    let entrance = [
        f32::from(entrance_cell[0]) + 0.5,
        0.0,
        f32::from(entrance_cell[1]) + 0.5,
    ];
    walls.push(MapBox {
        center: [entrance[0], 0.08, entrance[2]],
        size: [1.4, 0.16, 1.4],
        color: [0.86, 0.53, 0.16],
        solid: false,
    });
    WorldMap::from_parts(spawn, walls, triangles).map_err(GenerationError::materialization)
}

fn floor_triangles(x: u16, y: u16, color: [f32; 3]) -> [MapTriangle; 2] {
    let x = f32::from(x);
    let z = f32::from(y);
    [
        MapTriangle {
            vertices: [[x, 0.0, z], [x, 0.0, z + 1.0], [x + 1.0, 0.0, z]],
            precise_vertices: None,
            color,
            solid: true,
        },
        MapTriangle {
            vertices: [
                [x, 0.0, z + 1.0],
                [x + 1.0, 0.0, z + 1.0],
                [x + 1.0, 0.0, z],
            ],
            precise_vertices: None,
            color,
            solid: true,
        },
    ]
}

fn millimeters_to_world(position: [i64; 3]) -> [f32; 3] {
    position.map(|value| value as f32 / 1000.0)
}

fn radial_clearance(position: [i64; 3], distance_mm: i64) -> [i64; 3] {
    let length_squared = position
        .into_iter()
        .map(|coordinate| (coordinate * coordinate) as u64)
        .sum::<u64>();
    let length = integer_sqrt(length_squared) as i64;
    position.map(|coordinate| {
        let numerator = coordinate * distance_mm;
        let offset = if numerator < 0 {
            -((-numerator + length / 2) / length)
        } else {
            (numerator + length / 2) / length
        };
        coordinate + offset
    })
}

fn integer_sqrt(value: u64) -> u64 {
    if value < 2 {
        return value;
    }
    let mut current = value;
    let mut next = current.div_ceil(2);
    while next < current {
        current = next;
        next = (current + value / current) / 2;
    }
    current
}
