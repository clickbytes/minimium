//! Curved distant aggregates only. Local clouds use analytic instances.
mod lod;
use super::cloud_instances::shadow::Cache;
use bytemuck::{Pod, Zeroable};
use glam::DVec3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct Vertex {
    direction: [f32; 3],
    material_a: [f32; 4],
    material_b: [f32; 4],
    heights: [f32; 4],
    layer: f32,
}

const _: () = {
    assert!(std::mem::size_of::<Vertex>() == 64);
};
const MAX_VERTICES: usize = (lod::MAX_TILES + 6) * 3 * (lod::GRID + 1) * (lod::GRID + 1);
const MAX_INDICES: usize = (lod::MAX_TILES + 6) * 3 * lod::GRID * lod::GRID * 6;
const FALLBACK_INDICES: u32 = (6 * 3 * lod::GRID * lod::GRID * 6) as u32;

#[derive(Clone)]
struct Request {
    eye: DVec3,
    radius: f64,
    pixels_per_radian: f64,
    standard: bool,
    budget: usize,
}

impl Request {
    fn same_scene(&self, other: &Self) -> bool {
        self.radius == other.radius && self.eye.distance(other.eye) < self.radius * 0.1
    }
}

pub(super) struct Built {
    pub(super) vertices: Vec<Vertex>,
    pub(super) indices: Vec<u32>,
    pub(super) tiles: usize,
}

pub(super) fn build(
    eye: DVec3,
    radius: f64,
    pixels_per_radian: f64,
    budget: usize,
    cache: &Cache,
) -> Built {
    let request = Request {
        eye,
        radius,
        pixels_per_radian,
        standard: true,
        budget,
    };
    let tiles = lod::select(&request);
    let mut built = Built {
        vertices: Vec::new(),
        indices: Vec::new(),
        tiles: tiles.len(),
    };
    let fallback: Vec<_> = (0..6)
        .map(|face| lod::Tile {
            face,
            level: 0,
            x: 0,
            y: 0,
        })
        .collect();
    for tile in fallback.iter().chain(&tiles) {
        for layer in 0..3 {
            let start = built.vertices.len() as u32;
            for y in 0..=lod::GRID {
                for x in 0..=lod::GRID {
                    let up = lod::direction(
                        tile.face,
                        tile.uv(x as f64 / lod::GRID as f64, y as f64 / lod::GRID as f64),
                    );
                    let samples = [cache.sample(up, 0), cache.sample(up, 1)];
                    let material = |i: usize| {
                        [
                            samples[i].coverage[layer] as f32,
                            samples[i].optical_depth[layer] as f32,
                            samples[i].storm as f32,
                            0.0,
                        ]
                    };
                    built.vertices.push(Vertex {
                        direction: up.as_vec3().to_array(),
                        material_a: material(0),
                        material_b: material(1),
                        heights: [
                            samples[0].base_m[layer] as f32,
                            samples[0].top_m[layer] as f32,
                            samples[1].base_m[layer] as f32,
                            samples[1].top_m[layer] as f32,
                        ],
                        layer: layer as f32,
                    });
                }
            }
            for y in 0..lod::GRID {
                for x in 0..lod::GRID {
                    let a = start + (y * (lod::GRID + 1) + x) as u32;
                    let b = a + (lod::GRID + 1) as u32;
                    built
                        .indices
                        .extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
                }
            }
        }
    }
    built
}

pub(super) struct CloudMeshes {
    tiles: usize,
    pub(super) vertices: wgpu::Buffer,
    pub(super) indices: wgpu::Buffer,
    pub(super) index_count: u32,
}

impl CloudMeshes {
    pub(super) const BYTES: u64 =
        (MAX_VERTICES * std::mem::size_of::<Vertex>() + MAX_INDICES * 4) as u64;
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let buffer = |label, size, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage,
                mapped_at_creation: false,
            })
        };
        Self {
            tiles: 0,
            vertices: buffer(
                "curved cloud aggregates",
                (MAX_VERTICES * 64) as u64,
                wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            ),
            indices: buffer(
                "curved cloud indices",
                (MAX_INDICES * 4) as u64,
                wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            ),
            index_count: 0,
        }
    }
    pub(super) fn upload(&mut self, queue: &wgpu::Queue, built: &Built) {
        self.tiles = built.tiles;
        assert!(built.vertices.len() <= MAX_VERTICES && built.indices.len() <= MAX_INDICES);
        if !built.vertices.is_empty() {
            queue.write_buffer(&self.vertices, 0, bytemuck::cast_slice(&built.vertices));
        }
        if !built.indices.is_empty() {
            queue.write_buffer(&self.indices, 0, bytemuck::cast_slice(&built.indices));
        }
        self.index_count = built.indices.len() as u32;
    }
    pub(super) fn draw_range(&self, limit: usize) -> std::ops::Range<u32> {
        if self.index_count == 0 {
            return 0..0;
        }
        if self.tiles > limit {
            0..FALLBACK_INDICES
        } else {
            FALLBACK_INDICES..self.index_count
        }
    }
    pub(super) fn visible_tiles(&self, limit: usize) -> usize {
        if self.index_count == 0 {
            0
        } else if self.tiles > limit {
            6
        } else {
            self.tiles
        }
    }
    pub(super) fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32];
        wgpu::VertexBufferLayout {
            array_stride: 64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}
