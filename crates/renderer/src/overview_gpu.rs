//! GPU buffers for authored geometry and its overview marker.
use crate::geometry::Vertex;
use wgpu::util::DeviceExt;

#[derive(Default)]
pub struct OverviewGpuBuffers {
    world: Option<(wgpu::Buffer, u32)>,
    marker: Option<(wgpu::Buffer, u32)>,
}

impl OverviewGpuBuffers {
    pub fn world(&self) -> Option<&(wgpu::Buffer, u32)> {
        self.world.as_ref()
    }

    pub fn marker(&self) -> Option<&(wgpu::Buffer, u32)> {
        self.marker.as_ref()
    }

    pub fn install_world(&mut self, device: &wgpu::Device, vertices: &[Vertex]) -> (u64, u64) {
        let old_size = self.world.take().map_or(0, |(buffer, _)| buffer.size());
        let Some(buffer) = create_buffer(device, vertices, "world-overview-geometry") else {
            return (old_size, 0);
        };
        let new_size = buffer.size();
        self.world = Some((buffer, vertices.len() as u32));
        (old_size, new_size)
    }

    pub fn install_marker(&mut self, device: &wgpu::Device, vertices: &[Vertex]) -> (u64, u64) {
        let old_size = self.marker.take().map_or(0, |(buffer, _)| buffer.size());
        let Some(buffer) = create_buffer(device, vertices, "world-overview-marker") else {
            return (old_size, 0);
        };
        let new_size = buffer.size();
        self.marker = Some((buffer, vertices.len() as u32));
        (old_size, new_size)
    }
}

fn create_buffer(device: &wgpu::Device, vertices: &[Vertex], label: &str) -> Option<wgpu::Buffer> {
    (!vertices.is_empty()).then(|| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        })
    })
}
