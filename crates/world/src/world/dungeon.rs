use super::ObjectOriginId;
use std::collections::HashSet;

const MAX_DUNGEON_DIMENSION: u16 = 128;
const MAX_DUNGEON_ROOMS: usize = 32;

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

impl DungeonBlueprint {
    /// Validates authored or generated layout data before it enters a world.
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
            || !(3..=MAX_DUNGEON_ROOMS).contains(&rooms.len())
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
            || !blueprint.walkable_connected()
        {
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
            let neighbors = [
                (x > 0).then_some(index.wrapping_sub(1)),
                (x + 1 < usize::from(self.width)).then_some(index + 1),
                (y > 0).then_some(index.wrapping_sub(usize::from(self.width))),
                (y + 1 < usize::from(self.height)).then_some(index + usize::from(self.width)),
            ];
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
}
