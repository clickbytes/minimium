use super::*;
use minimium_core::world::{MapBox, MapTriangle};

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
    normal: [f32; 3],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];
    pub(super) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

impl UiVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x3];
    pub(super) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}
pub(super) fn sector_geometry(geometry: &minimium_procgen::SectorGeometry) -> Vec<Vertex> {
    let mut out = Vec::with_capacity(geometry.boxes.len() * 36 + geometry.triangles.len() * 3);
    append_geometry(&mut out, &geometry.boxes, &geometry.triangles);
    out
}

pub(super) fn sector_geometry_relative(
    geometry: &minimium_procgen::SectorGeometry,
    origin: [f32; 3],
) -> Vec<Vertex> {
    let mut vertices = sector_geometry(geometry);
    for vertex in &mut vertices {
        for (coordinate, origin) in vertex.position.iter_mut().zip(origin) {
            *coordinate = (f64::from(*coordinate) - f64::from(origin)) as f32;
        }
    }
    for (triangle, rendered) in geometry.triangles.iter().zip(
        vertices[geometry.boxes.len() * 36..]
            .as_chunks_mut::<3>()
            .0
            .iter_mut(),
    ) {
        if let Some(precise) = triangle.precise_vertices {
            for (point, vertex) in precise.iter().zip(rendered.iter_mut()) {
                vertex.position = std::array::from_fn(|axis| {
                    (point[axis] as f64 / 1000.0 - f64::from(origin[axis])) as f32
                });
            }
        }
    }
    vertices
}

fn append_geometry(out: &mut Vec<Vertex>, boxes: &[MapBox], triangles: &[MapTriangle]) {
    for map_box in boxes {
        cube(out, map_box.center, map_box.size, map_box.color);
    }
    // Preserve source triangle winding: procedural planet surfaces already face outward,
    // while authored and dungeon surfaces use their own upward/side-face semantics.
    for triangle in triangles {
        let points = triangle
            .precise_vertices
            .map(|vertices| vertices.map(|point| point.map(|value| value as f64 / 1000.0)))
            .unwrap_or_else(|| triangle.vertices.map(|point| point.map(f64::from)));
        let a = glam::DVec3::from_array(points[0]);
        let b = glam::DVec3::from_array(points[1]);
        let c = glam::DVec3::from_array(points[2]);
        let normal = (b - a)
            .cross(c - a)
            .normalize_or_zero()
            .as_vec3()
            .to_array();
        for position in triangle.vertices {
            out.push(Vertex {
                position,
                color: triangle.color,
                normal,
            });
        }
    }
}

pub(super) fn cube(out: &mut Vec<Vertex>, center: [f32; 3], size: [f32; 3], color: [f32; 3]) {
    let [cx, cy, cz] = center;
    let [sx, sy, sz] = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
    let p = [
        [cx - sx, cy - sy, cz - sz],
        [cx + sx, cy - sy, cz - sz],
        [cx + sx, cy + sy, cz - sz],
        [cx - sx, cy + sy, cz - sz],
        [cx - sx, cy - sy, cz + sz],
        [cx + sx, cy - sy, cz + sz],
        [cx + sx, cy + sy, cz + sz],
        [cx - sx, cy + sy, cz + sz],
    ];
    for face in [
        [0, 2, 1, 0, 3, 2],
        [5, 7, 4, 5, 6, 7],
        [4, 3, 0, 4, 7, 3],
        [1, 6, 5, 1, 2, 6],
        [3, 6, 2, 3, 7, 6],
        [4, 1, 5, 4, 0, 1],
    ] {
        let a = glam::Vec3::from_array(p[face[0]]);
        let b = glam::Vec3::from_array(p[face[1]]);
        let c = glam::Vec3::from_array(p[face[2]]);
        let normal = (b - a).cross(c - a).normalize_or_zero().to_array();
        for index in face {
            out.push(Vertex {
                position: p[index],
                color,
                normal,
            });
        }
    }
}
