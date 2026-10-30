use crate::{DungeonBlueprint, DungeonTile, PlanetLandmarks, PlanetTerrain};
use minimium_core::world::{
    MapBox, MapTriangle, PlanetFace, WorldMap, PLANET_SAMPLE_COORDINATE_SCALE,
};

const TERRAIN_COLOR: [f32; 3] = [0.18, 0.42, 0.19];
const ROOM_COLOR: [f32; 3] = [0.34, 0.32, 0.28];
const WALL_COLOR: [f32; 3] = [0.28, 0.30, 0.33];
const PLANET_SPAWN_CLEARANCE_MM: i64 = 2_000;

/// Produces a bounded triangulated landing region from canonical planet samples.
pub fn materialize_planet_map(
    terrain: PlanetTerrain,
    landmarks: PlanetLandmarks,
) -> Result<WorldMap, String> {
    let u_steps = 16_usize;
    // Half-sector steps put all nine route landmarks exactly on mesh vertices.
    let v_steps = 36_usize;
    let scale = PLANET_SAMPLE_COORDINATE_SCALE;
    let half_u = scale / 32;
    let v_start = -(scale / 8 + scale / 64);
    let v_span = scale / 4 + scale / 32;
    let mut points = Vec::with_capacity((u_steps + 1) * (v_steps + 1));
    for v_index in 0..=v_steps {
        let v = v_start + v_span * v_index as i32 / v_steps as i32;
        for u_index in 0..=u_steps {
            let u = -half_u + 2 * half_u * u_index as i32 / u_steps as i32;
            let sample = PlanetFace::PositiveY
                .cube_sample(u, v)
                .ok_or_else(|| String::from("planet sample is outside face bounds"))?;
            points.push(millimeters_to_world(
                terrain.sample(sample).position_mm.coordinates(),
            ));
        }
    }
    let mut triangles = Vec::with_capacity(u_steps * v_steps * 2);
    for v_index in 0..v_steps {
        for u_index in 0..u_steps {
            let lower_left = v_index * (u_steps + 1) + u_index;
            let lower_right = lower_left + 1;
            let upper_left = lower_left + u_steps + 1;
            let upper_right = upper_left + 1;
            triangles.push(MapTriangle {
                vertices: [points[lower_left], points[upper_left], points[lower_right]],
                color: TERRAIN_COLOR,
                solid: true,
            });
            triangles.push(MapTriangle {
                vertices: [points[lower_right], points[upper_left], points[upper_right]],
                color: TERRAIN_COLOR,
                solid: true,
            });
        }
    }
    let spawn = millimeters_to_world(radial_clearance(
        landmarks.spawn.position_mm.coordinates(),
        PLANET_SPAWN_CLEARANCE_MM,
    ));
    let entrance = millimeters_to_world(landmarks.entrance.position_mm.coordinates());
    let marker = MapBox {
        center: [entrance[0], entrance[1] + 0.6, entrance[2]],
        size: [0.65, 1.2, 0.65],
        color: [0.86, 0.53, 0.16],
        solid: false,
    };
    WorldMap::from_parts(spawn, vec![marker], triangles)
}

/// Turns a connected dungeon floor plan into shared render and collision geometry.
pub fn materialize_dungeon_map(blueprint: &DungeonBlueprint) -> Result<WorldMap, String> {
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
                    return Err(String::from(
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
    WorldMap::from_parts(spawn, walls, triangles)
}

fn floor_triangles(x: u16, y: u16, color: [f32; 3]) -> [MapTriangle; 2] {
    let x = f32::from(x);
    let z = f32::from(y);
    [
        MapTriangle {
            vertices: [[x, 0.0, z], [x, 0.0, z + 1.0], [x + 1.0, 0.0, z]],
            color,
            solid: true,
        },
        MapTriangle {
            vertices: [
                [x, 0.0, z + 1.0],
                [x + 1.0, 0.0, z + 1.0],
                [x + 1.0, 0.0, z],
            ],
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
