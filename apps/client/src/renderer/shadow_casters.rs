use super::shadows::ShadowBounds;
use minimium_core::world::{MapBox, MapTriangle, WorldMap};
use minimium_procgen::SectorGeometry;

/// Selects authored map geometry near the current visual caster origin.
/// A 300 m cache radius leaves room for 20 m scene refresh steps around the 256 m caster range.
pub(super) fn local_map_geometry(map: &WorldMap, center: [f64; 3], radius: f64) -> SectorGeometry {
    let boxes = map
        .boxes()
        .iter()
        .copied()
        .filter(|item| box_bounds(*item).intersects_sphere(center, radius))
        .collect();
    let triangles = map
        .triangles()
        .iter()
        .copied()
        .filter(|item| triangle_bounds(*item).intersects_sphere(center, radius))
        .collect();
    SectorGeometry { boxes, triangles }
}

fn box_bounds(item: MapBox) -> ShadowBounds {
    let half = item.size.map(|value| f64::from(value.abs()) * 0.5);
    let center = item.center.map(f64::from);
    ShadowBounds {
        minimum: std::array::from_fn(|axis| center[axis] - half[axis]),
        maximum: std::array::from_fn(|axis| center[axis] + half[axis]),
    }
}

fn triangle_bounds(item: MapTriangle) -> ShadowBounds {
    let vertices = item.precise_vertices.map_or_else(
        || item.vertices.map(|point| point.map(f64::from)),
        |points| points.map(|point| point.map(|value| value as f64 / 1000.0)),
    );
    let mut minimum = [f64::INFINITY; 3];
    let mut maximum = [f64::NEG_INFINITY; 3];
    for point in vertices {
        for axis in 0..3 {
            minimum[axis] = minimum[axis].min(point[axis]);
            maximum[axis] = maximum[axis].max(point[axis]);
        }
    }
    ShadowBounds { minimum, maximum }
}
