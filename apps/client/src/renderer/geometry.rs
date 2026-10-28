use super::*;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
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
pub(super) fn map_geometry(map: &WorldMap) -> Vec<Vertex> {
    let mut out = Vec::with_capacity(map.boxes().len() * 36 + map.triangles().len() * 3);
    for map_box in map.boxes() {
        cube(&mut out, map_box.center, map_box.size, map_box.color);
    }
    for triangle in map.triangles() {
        for position in triangle.vertices {
            out.push(Vertex {
                position,
                color: triangle.color,
            });
        }
    }
    out
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
        [0, 1, 2, 0, 2, 3],
        [5, 4, 7, 5, 7, 6],
        [4, 0, 3, 4, 3, 7],
        [1, 5, 6, 1, 6, 2],
        [3, 2, 6, 3, 6, 7],
        [4, 5, 1, 4, 1, 0],
    ] {
        for index in face {
            out.push(Vertex {
                position: p[index],
                color,
            });
        }
    }
}
