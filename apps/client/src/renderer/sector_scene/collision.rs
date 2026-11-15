use minimium_procgen::SectorGeometry;
use minimium_simulation::CollisionIndex;
use minimium_world::world::{Aabb, MapBox, MapTriangle};

pub(in crate::renderer) fn collision_from_sectors<'a>(
    sectors: impl Iterator<Item = &'a SectorGeometry>,
) -> CollisionIndex {
    let mut colliders = Vec::new();
    let mut seen_boxes = std::collections::HashSet::new();
    let mut triangles = Vec::new();
    let mut seen_triangles = std::collections::HashSet::new();
    for sector in sectors {
        for map_box in &sector.boxes {
            if map_box.solid && seen_boxes.insert(box_key(*map_box)) {
                colliders.push(Aabb::from_center_size(map_box.center, map_box.size));
            }
        }
        for triangle in sector.triangles.iter().chain(&sector.object_triangles) {
            if triangle.solid && seen_triangles.insert(triangle_key(*triangle)) {
                triangles.push(*triangle);
            }
        }
    }
    CollisionIndex::from_geometry(colliders, triangles)
}

fn box_key(map_box: MapBox) -> [u32; 6] {
    [
        map_box.center[0].to_bits(),
        map_box.center[1].to_bits(),
        map_box.center[2].to_bits(),
        map_box.size[0].to_bits(),
        map_box.size[1].to_bits(),
        map_box.size[2].to_bits(),
    ]
}

#[derive(Hash, Eq, PartialEq)]
enum TriangleKey {
    Precise([i64; 9]),
    Float([u32; 9]),
}

fn triangle_key(triangle: MapTriangle) -> TriangleKey {
    if let Some(vertices) = triangle.precise_vertices {
        return TriangleKey::Precise(std::array::from_fn(|index| vertices[index / 3][index % 3]));
    }
    let [a, b, c] = triangle.vertices;
    TriangleKey::Float([
        a[0].to_bits(),
        a[1].to_bits(),
        a[2].to_bits(),
        b[0].to_bits(),
        b[1].to_bits(),
        b[2].to_bits(),
        c[0].to_bits(),
        c[1].to_bits(),
        c[2].to_bits(),
    ])
}
