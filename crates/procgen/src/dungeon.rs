use crate::{sector_neighborhood::DUNGEON_SECTOR_EDGE, AddressedRandom, GenerationChannel};
use minimium_world::world::{
    DungeonBlueprint, DungeonRoom, DungeonSectorKey, DungeonTile, GenerationSpec, SectorKey,
};

const MAX_DUNGEON_DIMENSION: u16 = 128;
const MAX_MINIMUM_ROOM_EDGE: u16 = 40;
const MAX_DUNGEON_ROOMS: u8 = 32;
const MAX_BSP_DEPTH: u8 = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DungeonSpec {
    width: u16,
    height: u16,
    minimum_room_edge: u16,
    maximum_rooms: u8,
    corridor_width: u8,
}

impl DungeonSpec {
    pub fn new(
        width: u16,
        height: u16,
        minimum_room_edge: u16,
        maximum_rooms: u8,
        corridor_width: u8,
    ) -> Option<Self> {
        if !(3..=MAX_DUNGEON_DIMENSION).contains(&width)
            || !(3..=MAX_DUNGEON_DIMENSION).contains(&height)
            || !(3..=MAX_MINIMUM_ROOM_EDGE).contains(&minimum_room_edge)
            || width < minimum_room_edge + 4
            || height < minimum_room_edge + 4
            || (width - 2 < (minimum_room_edge + 2) * 3 && height - 2 < (minimum_room_edge + 2) * 3)
            || !(3..=MAX_DUNGEON_ROOMS).contains(&maximum_rooms)
            || !(corridor_width == 1 || corridor_width == 3)
        {
            return None;
        }
        Some(Self {
            width,
            height,
            minimum_room_edge,
            maximum_rooms,
            corridor_width,
        })
    }
}

struct DungeonBuilder {
    width: u16,
    height: u16,
    tiles: Vec<DungeonTile>,
    rooms: Vec<DungeonRoom>,
}

#[derive(Clone, Debug)]
pub struct DungeonSector {
    pub key: DungeonSectorKey,
    pub edge: u16,
    tiles: Vec<DungeonTile>,
}

#[derive(Clone, Copy)]
struct Rect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

#[derive(Clone, Copy)]
struct Point {
    x: u16,
    y: u16,
}

/// Builds a bounded BSP floor plan and carves one corridor across every split.
pub fn generate_dungeon(generation: GenerationSpec, spec: DungeonSpec) -> DungeonBlueprint {
    let mut builder = DungeonBuilder {
        width: spec.width,
        height: spec.height,
        tiles: vec![DungeonTile::Void; usize::from(spec.width) * usize::from(spec.height)],
        rooms: Vec::with_capacity(usize::from(spec.maximum_rooms)),
    };
    let root = Rect {
        x: 1,
        y: 1,
        width: spec.width - 2,
        height: spec.height - 2,
    };
    carve_partition(
        generation,
        spec,
        root,
        0,
        0,
        spec.maximum_rooms,
        &mut builder,
    );
    DungeonBlueprint::from_parts(builder.width, builder.height, builder.tiles, builder.rooms)
        .expect("procedural dungeon builder produces a valid connected layout")
}

pub fn materialize_dungeon_sector(
    blueprint: &DungeonBlueprint,
    key: DungeonSectorKey,
) -> Option<DungeonSector> {
    let edge = DUNGEON_SECTOR_EDGE;
    let mut tiles = vec![DungeonTile::Void; usize::from(edge) * usize::from(edge)];
    let (sector_x, sector_y, sector_z) = key.coordinates();
    if sector_y == 0 {
        for local_z in 0..edge {
            for local_x in 0..edge {
                let global_x = i64::from(sector_x) * i64::from(edge) + i64::from(local_x);
                let global_z = i64::from(sector_z) * i64::from(edge) + i64::from(local_z);
                if global_x >= 0
                    && global_z >= 0
                    && global_x < i64::from(blueprint.dimensions().0)
                    && global_z < i64::from(blueprint.dimensions().1)
                {
                    if let Some(tile) = blueprint.tile(global_x as u16, global_z as u16) {
                        let index = usize::from(local_z) * usize::from(edge) + usize::from(local_x);
                        tiles[index] = tile;
                    }
                }
            }
        }
    }
    Some(DungeonSector { key, edge, tiles })
}

impl DungeonSector {
    pub fn tile(&self, x: u16, z: u16) -> Option<DungeonTile> {
        if x >= self.edge || z >= self.edge {
            return None;
        }
        Some(self.tiles[usize::from(z) * usize::from(self.edge) + usize::from(x)])
    }
}

fn carve_partition(
    generation: GenerationSpec,
    spec: DungeonSpec,
    bounds: Rect,
    depth: u8,
    path: u64,
    budget: u8,
    blueprint: &mut DungeonBuilder,
) -> Point {
    let key = node_key(depth, path);
    let random = AddressedRandom::new(generation, GenerationChannel::DungeonLayout, key);
    let minimum_partition = spec.minimum_room_edge + 2;
    let can_split_x = bounds.width >= minimum_partition * 2;
    let can_split_y = bounds.height >= minimum_partition * 2;
    let can_three_x = bounds.width >= minimum_partition * 3;
    let can_three_y = bounds.height >= minimum_partition * 3;
    if budget <= 1 || depth >= MAX_BSP_DEPTH || (!can_split_x && !can_split_y) {
        return carve_room(random, bounds, spec.minimum_room_edge, blueprint);
    }

    let split_vertical = match (budget >= 3 && can_three_x, budget >= 3 && can_three_y) {
        (true, false) => true,
        (false, true) => false,
        (true, true) if bounds.width > bounds.height * 5 / 4 => true,
        (true, true) if bounds.height > bounds.width * 5 / 4 => false,
        (true, true) => random.sample_u32(0) & 1 == 0,
        _ => match (can_split_x, can_split_y) {
            (true, false) => true,
            (false, true) => false,
            (true, true) if bounds.width > bounds.height * 5 / 4 => true,
            (true, true) if bounds.height > bounds.width * 5 / 4 => false,
            (true, true) => random.sample_u32(0) & 1 == 0,
            _ => return carve_room(random, bounds, spec.minimum_room_edge, blueprint),
        },
    };

    let extent = if split_vertical {
        bounds.width
    } else {
        bounds.height
    };
    let preserve_split = budget >= 3
        && if split_vertical {
            can_three_x
        } else {
            can_three_y
        };
    let maximum_first = if preserve_split {
        extent - minimum_partition * 2
    } else {
        extent - minimum_partition
    };
    let first_min = minimum_partition;
    let split_count = maximum_first - minimum_partition + 1;
    let split = first_min + (random.sample_u32(1) % u32::from(split_count)) as u16;
    let (first, second) = if split_vertical {
        (
            Rect {
                width: split,
                ..bounds
            },
            Rect {
                x: bounds.x + split,
                width: bounds.width - split,
                ..bounds
            },
        )
    } else {
        (
            Rect {
                height: split,
                ..bounds
            },
            Rect {
                y: bounds.y + split,
                height: bounds.height - split,
                ..bounds
            },
        )
    };
    let first_budget = budget / 2;
    let first_point = carve_partition(
        generation,
        spec,
        first,
        depth + 1,
        path << 1,
        first_budget,
        blueprint,
    );
    let second_point = carve_partition(
        generation,
        spec,
        second,
        depth + 1,
        path << 1 | 1,
        budget - first_budget,
        blueprint,
    );
    carve_corridor(blueprint, first_point, second_point, spec.corridor_width);
    first_point
}

fn carve_room(
    random: AddressedRandom,
    bounds: Rect,
    minimum_edge: u16,
    blueprint: &mut DungeonBuilder,
) -> Point {
    let available_width = bounds.width - 2;
    let available_height = bounds.height - 2;
    let width = minimum_edge
        + (random.sample_u32(0) % u32::from(available_width - minimum_edge + 1)) as u16;
    let height = minimum_edge
        + (random.sample_u32(1) % u32::from(available_height - minimum_edge + 1)) as u16;
    let x_range = bounds.width - width - 1;
    let y_range = bounds.height - height - 1;
    let x = bounds.x + 1 + (random.sample_u32(2) % u32::from(x_range)) as u16;
    let y = bounds.y + 1 + (random.sample_u32(3) % u32::from(y_range)) as u16;
    let center = Point {
        x: x + width / 2,
        y: y + height / 2,
    };
    let origin_id = random.object_origin(0);
    let room = DungeonRoom {
        origin_id,
        x,
        y,
        width,
        height,
        center: [center.x, center.y],
    };
    for row in y..y + height {
        for col in x..x + width {
            set_tile(blueprint, col, row, DungeonTile::Room);
        }
    }
    blueprint.rooms.push(room);
    center
}

fn carve_corridor(blueprint: &mut DungeonBuilder, from: Point, to: Point, width: u8) {
    let radius = i32::from(width) / 2;
    let horizontal_start = from.x.min(to.x);
    let horizontal_end = from.x.max(to.x);
    for x in horizontal_start..=horizontal_end {
        for offset in -radius..=radius {
            let y = i32::from(from.y) + offset;
            if y >= 0 {
                set_tile(blueprint, x, y as u16, DungeonTile::Corridor);
            }
        }
    }
    let vertical_start = from.y.min(to.y);
    let vertical_end = from.y.max(to.y);
    for y in vertical_start..=vertical_end {
        for offset in -radius..=radius {
            let x = i32::from(to.x) + offset;
            if x >= 0 {
                set_tile(blueprint, x as u16, y, DungeonTile::Corridor);
            }
        }
    }
}

fn set_tile(blueprint: &mut DungeonBuilder, x: u16, y: u16, tile: DungeonTile) {
    if x >= blueprint.width || y >= blueprint.height {
        return;
    }
    let index = usize::from(y) * usize::from(blueprint.width) + usize::from(x);
    if blueprint.tiles[index] == DungeonTile::Void || tile == DungeonTile::Room {
        blueprint.tiles[index] = tile;
    }
}

fn node_key(depth: u8, path: u64) -> SectorKey {
    SectorKey::dungeon(
        path as u32 as i32,
        (path >> 32) as u32 as i32,
        i32::from(depth),
    )
}
