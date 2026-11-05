//! Reusable dynamic vertex buffer owned by the renderer.
use crate::geometry::Vertex;
use wgpu::util::DeviceExt;

pub struct StaticVertexBuffer {
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

impl StaticVertexBuffer {
    pub fn new(device: &wgpu::Device, label: &str, vertices: &[Vertex]) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Self {
            buffer,
            vertex_count: vertices.len() as u32,
        }
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    pub fn size_bytes(&self) -> u64 {
        self.buffer.size()
    }
}
pub struct DynamicVertexBuffer {
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

impl DynamicVertexBuffer {
    pub fn new(device: &wgpu::Device, label: &str, capacity: usize) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (capacity * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            vertex_count: 0,
        }
    }

    pub fn upload(&mut self, queue: &wgpu::Queue, vertices: &[Vertex]) {
        self.vertex_count = vertices.len() as u32;
        if !vertices.is_empty() {
            queue.write_buffer(&self.buffer, 0, bytemuck::cast_slice(vertices));
        }
    }

    pub fn clear(&mut self) {
        self.vertex_count = 0;
    }

    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }

    pub fn size_bytes(&self) -> u64 {
        self.buffer.size()
    }
}
