use super::collision_index::{Aabb, CollisionIndex};
use super::geometry::{intersects_player, triangle_area_squared, triangle_intersects_player};

pub const MAX_MAP_BOXES: usize = 4096;
pub const MAX_MAP_TRIANGLES: usize = 8192;
pub const MAX_MAP_COORDINATE: f32 = 1_000_000.0;

#[derive(Clone, Copy, Debug)]
pub struct MapBox {
    pub center: [f32; 3],
    pub size: [f32; 3],
    pub color: [f32; 3],
    pub solid: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct MapTriangle {
    pub vertices: [[f32; 3]; 3],
    pub color: [f32; 3],
    pub solid: bool,
}

#[derive(Clone, Debug)]
pub struct WorldMap {
    pub(super) spawn: [f32; 3],
    pub(super) boxes: Vec<MapBox>,
    pub(super) triangles: Vec<MapTriangle>,
    pub(super) fingerprint: u64,
}

impl WorldMap {
    /// Constructs a map only after validating its domain geometry.
    pub fn from_parts(
        spawn: [f32; 3],
        boxes: Vec<MapBox>,
        triangles: Vec<MapTriangle>,
    ) -> Result<Self, String> {
        let coordinate = |value: f32| value.is_finite() && value.abs() <= MAX_MAP_COORDINATE;
        let color = |value: f32| (0.0..=1.0).contains(&value);
        if !spawn.into_iter().all(coordinate)
            || boxes.len() > MAX_MAP_BOXES
            || triangles.len() > MAX_MAP_TRIANGLES
            || (boxes.is_empty() && triangles.is_empty())
            || boxes.iter().any(|item| {
                !item.center.into_iter().all(coordinate)
                    || !item.size.into_iter().all(|v| coordinate(v) && v > 0.0)
                    || !item.color.into_iter().all(color)
            })
            || triangles.iter().any(|item| {
                !item.vertices.into_iter().flatten().all(coordinate)
                    || !item.color.into_iter().all(color)
                    || triangle_area_squared(item.vertices) <= 1.0e-12
            })
        {
            return Err("invalid map geometry".into());
        }
        if boxes
            .iter()
            .filter(|item| item.solid)
            .any(|item| intersects_player(spawn, Aabb::from_center_size(item.center, item.size)))
            || triangles
                .iter()
                .filter(|item| item.solid)
                .any(|item| triangle_intersects_player(spawn, *item))
        {
            return Err("spawn overlaps solid map geometry".into());
        }
        let fingerprint = map_fingerprint(spawn, &boxes, &triangles);
        Ok(Self {
            spawn,
            boxes,
            triangles,
            fingerprint,
        })
    }

    pub fn spawn(&self) -> [f32; 3] {
        self.spawn
    }

    pub fn boxes(&self) -> &[MapBox] {
        &self.boxes
    }

    pub fn triangles(&self) -> &[MapTriangle] {
        &self.triangles
    }

    pub fn solid_colliders(&self) -> Vec<Aabb> {
        self.boxes
            .iter()
            .filter(|map_box| map_box.solid)
            .map(|map_box| Aabb::from_center_size(map_box.center, map_box.size))
            .collect()
    }

    pub fn solid_triangles(&self) -> Vec<MapTriangle> {
        self.triangles
            .iter()
            .copied()
            .filter(|triangle| triangle.solid)
            .collect()
    }

    pub fn collision_index(&self) -> CollisionIndex {
        CollisionIndex::from_geometry(self.solid_colliders(), self.solid_triangles())
    }

    pub fn can_place_player(&self, position: [f32; 3]) -> bool {
        position.iter().all(|value| value.is_finite())
            && self
                .boxes
                .iter()
                .filter(|map_box| map_box.solid)
                .all(|map_box| {
                    !intersects_player(
                        position,
                        Aabb::from_center_size(map_box.center, map_box.size),
                    )
                })
            && self
                .triangles
                .iter()
                .filter(|triangle| triangle.solid)
                .all(|triangle| !triangle_intersects_player(position, *triangle))
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

pub(super) fn map_fingerprint(spawn: [f32; 3], boxes: &[MapBox], triangles: &[MapTriangle]) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    feed(b"MINIMIUM_MAP 1");
    for value in spawn {
        feed(&canonical_float_bits(value).to_le_bytes());
    }
    for map_box in boxes {
        for value in map_box
            .center
            .into_iter()
            .chain(map_box.size)
            .chain(map_box.color)
        {
            feed(&canonical_float_bits(value).to_le_bytes());
        }
        feed(&[u8::from(map_box.solid)]);
    }
    if !triangles.is_empty() {
        feed(b"\0TRIANGLES\0");
        feed(&(triangles.len() as u32).to_le_bytes());
        for triangle in triangles {
            for value in triangle
                .vertices
                .into_iter()
                .flatten()
                .chain(triangle.color)
            {
                feed(&canonical_float_bits(value).to_le_bytes());
            }
            feed(&[u8::from(triangle.solid)]);
        }
    }
    hash
}

pub(super) fn canonical_float_bits(value: f32) -> u32 {
    if value == 0.0 {
        0.0f32.to_bits()
    } else {
        value.to_bits()
    }
}
