//! Spatial bounds and local geometry selection used by the shadow renderer.
use minimium_procgen::SectorGeometry;
use minimium_world::world::{MapBox, MapTriangle, WorldMap};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowBounds {
    pub minimum: [f64; 3],
    pub maximum: [f64; 3],
}

impl ShadowBounds {
    pub fn intersects_sphere(self, center: [f64; 3], radius: f64) -> bool {
        self.distance_to(center) <= radius
    }

    pub fn distance_to(self, center: [f64; 3]) -> f64 {
        let distance_squared = (0..3)
            .map(|axis| {
                let outside = if center[axis] < self.minimum[axis] {
                    self.minimum[axis] - center[axis]
                } else if center[axis] > self.maximum[axis] {
                    center[axis] - self.maximum[axis]
                } else {
                    0.0
                };
                outside * outside
            })
            .sum::<f64>();
        distance_squared.sqrt()
    }
}

pub fn geometry_bounds(geometry: &SectorGeometry) -> Option<ShadowBounds> {
    let mut bounds = EmptyBounds::new();
    for map_box in &geometry.boxes {
        let half = map_box.size.map(|value| f64::from(value.abs()) * 0.5);
        let center = map_box.center.map(f64::from);
        bounds.include(std::array::from_fn(|axis| center[axis] - half[axis]));
        bounds.include(std::array::from_fn(|axis| center[axis] + half[axis]));
    }
    for triangle in &geometry.triangles {
        let points = triangle.precise_vertices.map_or_else(
            || triangle.vertices.map(|point| point.map(f64::from)),
            |vertices| vertices.map(|point| point.map(|value| value as f64 / 1000.0)),
        );
        for point in points {
            bounds.include(point);
        }
    }
    bounds.finish()
}

struct EmptyBounds {
    minimum: [f64; 3],
    maximum: [f64; 3],
}

impl EmptyBounds {
    fn new() -> Self {
        Self {
            minimum: [f64::INFINITY; 3],
            maximum: [f64::NEG_INFINITY; 3],
        }
    }

    fn include(&mut self, point: [f64; 3]) {
        for (axis, coordinate) in point.into_iter().enumerate() {
            self.minimum[axis] = self.minimum[axis].min(coordinate);
            self.maximum[axis] = self.maximum[axis].max(coordinate);
        }
    }

    fn finish(self) -> Option<ShadowBounds> {
        (0..3)
            .all(|axis| self.minimum[axis].is_finite() && self.maximum[axis].is_finite())
            .then_some(ShadowBounds {
                minimum: self.minimum,
                maximum: self.maximum,
            })
    }
}

/// Selects authored map geometry near the current visual caster origin.
/// A 300 m cache radius leaves room for 20 m scene refresh steps around the 256 m caster range.
pub fn local_map_geometry(map: &WorldMap, center: [f64; 3], radius: f64) -> SectorGeometry {
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
