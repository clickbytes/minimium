//! Exact cube-face sector clipping for millimetre precise planet triangles.
use minimium_world::world::{MapSurfaceKind, MapSurfaceVertex, MapTriangle, PlanetFace};

pub(crate) fn clip_triangles_region(
    triangles: &[MapTriangle],
    face: PlanetFace,
    u: u16,
    v: u16,
    span: u16,
    count: u16,
) -> Vec<MapTriangle> {
    let (normal, u_axis, v_axis) = basis(face);
    let count = f64::from(count);
    let u_min = -1.0 + 2.0 * f64::from(u) / count;
    let u_max = -1.0 + 2.0 * f64::from(u + span) / count;
    let v_min = -1.0 + 2.0 * f64::from(v) / count;
    let v_max = -1.0 + 2.0 * f64::from(v + span) / count;
    let planes = [
        normal.map(f64::from),
        combine(u_axis, normal, -u_min),
        combine(normal, u_axis, u_max),
        combine(v_axis, normal, -v_min),
        combine(normal, v_axis, v_max),
    ];
    let mut output = Vec::new();
    for triangle in triangles {
        let Some(precise) = triangle.precise_vertices else {
            output.push(*triangle);
            continue;
        };
        let mut polygon: Vec<_> = (0..3)
            .map(|i| Vertex {
                point: precise[i].map(|v| v as f64),
                surface: triangle.surface.map(|items| items[i]),
            })
            .collect();
        for plane in planes {
            polygon = clip_plane(polygon, plane);
        }
        if polygon.len() < 3 {
            continue;
        }
        for index in 1..polygon.len() - 1 {
            let vertices = [polygon[0], polygon[index], polygon[index + 1]];
            let points = vertices.map(|v| v.point.map(|c| c.round() as i64));
            if area_squared(points) < 1.0 {
                continue;
            }
            output.push(MapTriangle {
                vertices: points.map(|p| p.map(|c| c as f32 / 1000.0)),
                precise_vertices: Some(points),
                surface: if vertices.iter().all(|v| v.surface.is_some()) {
                    Some(vertices.map(|v| v.surface.unwrap()))
                } else {
                    None
                },
                ..*triangle
            });
        }
    }
    output
}

#[derive(Clone, Copy)]
struct Vertex {
    point: [f64; 3],
    surface: Option<MapSurfaceVertex>,
}

fn clip_plane(polygon: Vec<Vertex>, plane: [f64; 3]) -> Vec<Vertex> {
    let mut result = Vec::new();
    for index in 0..polygon.len() {
        let a = polygon[index];
        let b = polygon[(index + 1) % polygon.len()];
        let da = dot(a.point, plane);
        let db = dot(b.point, plane);
        let a_inside = da >= -1.0e-6;
        let b_inside = db >= -1.0e-6;
        if a_inside {
            result.push(a);
        }
        if a_inside != b_inside {
            let t = (da / (da - db)).clamp(0.0, 1.0);
            result.push(Vertex {
                point: std::array::from_fn(|i| a.point[i] + (b.point[i] - a.point[i]) * t),
                surface: match (a.surface, b.surface) {
                    (Some(x), Some(y)) => Some(interpolate(x, y, t as f32)),
                    _ => None,
                },
            });
        }
    }
    result.dedup_by(|a, b| (0..3).all(|i| (a.point[i] - b.point[i]).abs() < 1.0e-7));
    result
}

pub(crate) fn interpolate(a: MapSurfaceVertex, b: MapSurfaceVertex, t: f32) -> MapSurfaceVertex {
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    let normal = std::array::from_fn(|i| lerp(a.normal[i], b.normal[i]));
    let length = normal.iter().map(|v| v * v).sum::<f32>().sqrt().max(1.0e-8);
    MapSurfaceVertex {
        kind: if matches!(a.kind, MapSurfaceKind::Excavation)
            || matches!(b.kind, MapSurfaceKind::Excavation)
        {
            MapSurfaceKind::Excavation
        } else {
            MapSurfaceKind::PlanetGround
        },
        normal: normal.map(|v| v / length),
        coverage: std::array::from_fn(|i| lerp(a.coverage[i], b.coverage[i])),
        source_elevation_m: lerp(a.source_elevation_m, b.source_elevation_m),
        surface_elevation_m: lerp(a.surface_elevation_m, b.surface_elevation_m),
        slope: lerp(a.slope, b.slope),
        water: lerp(a.water, b.water),
        habitat: std::array::from_fn(|i| lerp(a.habitat[i], b.habitat[i])),
    }
}

fn combine(a: [i8; 3], b: [i8; 3], scale: f64) -> [f64; 3] {
    std::array::from_fn(|i| f64::from(a[i]) + f64::from(b[i]) * scale)
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

fn basis(face: PlanetFace) -> ([i8; 3], [i8; 3], [i8; 3]) {
    match face {
        PlanetFace::PositiveX => ([1, 0, 0], [0, 1, 0], [0, 0, 1]),
        PlanetFace::NegativeX => ([-1, 0, 0], [0, 1, 0], [0, 0, 1]),
        PlanetFace::PositiveY => ([0, 1, 0], [1, 0, 0], [0, 0, 1]),
        PlanetFace::NegativeY => ([0, -1, 0], [1, 0, 0], [0, 0, 1]),
        PlanetFace::PositiveZ => ([0, 0, 1], [1, 0, 0], [0, 1, 0]),
        PlanetFace::NegativeZ => ([0, 0, -1], [1, 0, 0], [0, 1, 0]),
    }
}

fn area_squared([a, b, c]: [[i64; 3]; 3]) -> f64 {
    let ab = std::array::from_fn::<_, 3, _>(|i| (b[i] - a[i]) as f64);
    let ac = std::array::from_fn::<_, 3, _>(|i| (c[i] - a[i]) as f64);
    let n = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    n.iter().map(|v| v * v).sum()
}
