//! Shared GPU buffers and bind group for frame camera/environment uniforms.

use crate::gpu_frame::{CameraUniform, EnvironmentUniform};
use wgpu::util::DeviceExt;

/// Device resources backing the shared world-frame uniform bindings.
pub struct EnvironmentGpuBindings {
    /// Bind-group layout used by world, sky and globe pipelines.
    pub layout: wgpu::BindGroupLayout,
    /// Camera uniform buffer.
    pub camera_buffer: wgpu::Buffer,
    /// Environment uniform buffer.
    pub environment_buffer: wgpu::Buffer,
    /// Bind group containing both uniform buffers.
    pub bind_group: wgpu::BindGroup,
}

impl EnvironmentGpuBindings {
    /// Creates the shared camera/environment buffers and bind group.
    pub fn create(device: &wgpu::Device) -> Self {
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera-uniform"),
            contents: bytemuck::bytes_of(&CameraUniform::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let environment_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("environment-uniform"),
            contents: bytemuck::bytes_of(&EnvironmentUniform::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-frame-layout"),
            entries: &[
                uniform_entry(0, std::mem::size_of::<CameraUniform>() as u64),
                uniform_entry(1, std::mem::size_of::<EnvironmentUniform>() as u64),
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world-frame-bind-group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: environment_buffer.as_entire_binding(),
                },
            ],
        });
        Self {
            layout,
            camera_buffer,
            environment_buffer,
            bind_group,
        }
    }
}

fn uniform_entry(binding: u32, size: u64) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(size),
        },
        count: None,
    }
}
