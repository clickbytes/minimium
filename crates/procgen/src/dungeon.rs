use crate::{sector_neighborhood::DUNGEON_SECTOR_EDGE, AddressedRandom, GenerationChannel};
use minimium_core::world::{DungeonSectorKey, GenerationSpec, ObjectOriginId, SectorKey};
use std::collections::HashSet;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DungeonRoom {
    pub origin_id: ObjectOriginId,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub center: [u16; 2],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DungeonTile {
    Void = 0,
    Room = 1,
    Corridor = 2,
}

#[derive(Clone, Debug)]
pub struct DungeonBlueprint {
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
    let mut blueprint = DungeonBlueprint {
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
        &mut blueprint,
    );
    blueprint
}

impl DungeonBlueprint {
    /// Validates a hand-authored floor plan into the same materialization type.
    pub fn from_parts(
        width: u16,
        height: u16,
        tiles: Vec<DungeonTile>,
        rooms: Vec<DungeonRoom>,
    ) -> Option<Self> {
        if width < 3
            || height < 3
            || width > MAX_DUNGEON_DIMENSION
            || height > MAX_DUNGEON_DIMENSION
            || tiles.len() != usize::from(width) * usize::from(height)
            || !(3..=usize::from(MAX_DUNGEON_ROOMS)).contains(&rooms.len())
        {
            return None;
        }
        let blueprint = Self {
            width,
            height,
            tiles,
            rooms,
        };
        if blueprint.rooms.iter().any(|room| {
            room.width < 3
                || room.height < 3
                || u32::from(room.x) + u32::from(room.width) > u32::from(width)
                || u32::from(room.y) + u32::from(room.height) > u32::from(height)
                || room.center != [room.x + room.width / 2, room.y + room.height / 2]
                || blueprint.tile(room.center[0], room.center[1]) != Some(DungeonTile::Room)
                || (room.y..room.y + room.height).any(|y| {
                    (room.x..room.x + room.width)
                        .any(|x| blueprint.tile(x, y) != Some(DungeonTile::Room))
                })
        }) {
            return None;
        }
        let mut origin_ids = HashSet::with_capacity(blueprint.rooms.len());
        if blueprint
            .rooms
            .iter()
            .any(|room| !origin_ids.insert(room.origin_id))
            || blueprint.rooms.iter().enumerate().any(|(index, room)| {
                blueprint.rooms[index + 1..].iter().any(|other| {
                    u32::from(room.x) < u32::from(other.x) + u32::from(other.width)
                        && u32::from(other.x) < u32::from(room.x) + u32::from(room.width)
                        && u32::from(room.y) < u32::from(other.y) + u32::from(other.height)
                        && u32::from(other.y) < u32::from(room.y) + u32::from(room.height)
                })
            })
        {
            return None;
        }
        if !blueprint.walkable_connected() {
            return None;
        }
        Some(blueprint)
    }

    pub const fn dimensions(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    pub fn rooms(&self) -> &[DungeonRoom] {
        &self.rooms
    }

    pub fn tile(&self, x: u16, y: u16) -> Option<DungeonTile> {
        if x >= self.width || y >= self.height {
            return None;
        }
        Some(self.tiles[usize::from(y) * usize::from(self.width) + usize::from(x)])
    }

    pub fn spawn(&self) -> [u16; 2] {
        self.rooms[0].center
    }

    pub fn entrance(&self) -> [u16; 2] {
        self.rooms[self.rooms.len() - 1].center
    }

    fn walkable_connected(&self) -> bool {
        let Some(start) = self
            .tiles
            .iter()
            .position(|tile| *tile != DungeonTile::Void)
        else {
            return false;
        };
        let mut visited = vec![false; self.tiles.len()];
        let mut pending = vec![start];
        visited[start] = true;
        while let Some(index) = pending.pop() {
            let x = index % usize::from(self.width);
            let y = index / usize::from(self.width);
            let mut neighbors = [None; 4];
            if x > 0 {
                neighbors[0] = Some(index - 1);
            }
            if x + 1 < usize::from(self.width) {
                neighbors[1] = Some(index + 1);
            }
            if y > 0 {
                neighbors[2] = Some(index - usize::from(self.width));
            }
            if y + 1 < usize::from(self.height) {
                neighbors[3] = Some(index + usize::from(self.width));
            }
            for neighbor in neighbors.into_iter().flatten() {
                if !visited[neighbor] && self.tiles[neighbor] != DungeonTile::Void {
                    visited[neighbor] = true;
                    pending.push(neighbor);
                }
            }
        }
        self.tiles
            .iter()
            .zip(visited)
            .all(|(tile, seen)| *tile == DungeonTile::Void || seen)
    }

    /// Copies the plan into one fixed-size local 3D sector. This floor-plan
    /// revision places walkable cells at world y=0; other vertical sectors are empty.
    pub fn materialize_sector(&self, key: DungeonSectorKey) -> Option<DungeonSector> {
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
                        && global_x < i64::from(self.width)
                        && global_z < i64::from(self.height)
                    {
                        let index = usize::from(local_z) * usize::from(edge) + usize::from(local_x);
                        tiles[index] = self.tile(global_x as u16, global_z as u16)?;
                    }
                }
            }
        }
        Some(DungeonSector { key, edge, tiles })
    }
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
    blueprint: &mut DungeonBlueprint,
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
    blueprint: &mut DungeonBlueprint,
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

fn carve_corridor(blueprint: &mut DungeonBlueprint, from: Point, to: Point, width: u8) {
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

fn set_tile(blueprint: &mut DungeonBlueprint, x: u16, y: u16, tile: DungeonTile) {
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
