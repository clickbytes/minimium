//! Clear-air globe lighting, independent of weather and simulation workers.
mod pipeline;
mod profile;
mod render;
mod table;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C, align(16))]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct AtmosphereUniform {
    radii: [f32; 4],
    rayleigh: [f32; 4],
    mie: [f32; 4],
    ozone: [f32; 4],
    config: [f32; 4],
}

const _: () = {
    assert!(std::mem::size_of::<AtmosphereUniform>() == 80);
    assert!(std::mem::align_of::<AtmosphereUniform>() == 16);
};

pub(super) struct AtmosphereGpu {
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) depth_bind_group: wgpu::BindGroup,
    depth_layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
}

impl AtmosphereGpu {
    pub(super) const BYTES: u64 = table::BYTES + std::mem::size_of::<AtmosphereUniform>() as u64;

    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        depth: &wgpu::TextureView,
    ) -> Self {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("clear-air atmosphere uniform"),
            contents: bytemuck::bytes_of(&AtmosphereUniform::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let table = table::create(device, queue, profile::EARTH);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atmosphere table sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("clear-air atmosphere layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(80),
                    },
                    count: None,
                },
                table::entry(),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("clear-air atmosphere"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&table),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("atmosphere scene depth layout"),
            entries: &[pipeline::depth_entry()],
        });
        let depth_bind_group = pipeline::depth_group(device, &depth_layout, depth);
        let pipeline = pipeline::create(device, frame_layout, &layout, &depth_layout, format);
        Self {
            layout,
            bind_group,
            pipeline,
            depth_bind_group,
            depth_layout,
            uniform,
        }
    }

    pub(super) fn upload(&self, queue: &wgpu::Queue, radius: f32, standard_quality: bool) {
        let p = profile::EARTH;
        let value = AtmosphereUniform {
            radii: [
                p.radius_m as f32,
                p.atmospheric_top_m as f32,
                p.rayleigh_scale_height_m as f32,
                p.aerosol_scale_height_m as f32,
            ],
            rayleigh: [
                p.rayleigh_rgb_m_inv[0] as f32,
                p.rayleigh_rgb_m_inv[1] as f32,
                p.rayleigh_rgb_m_inv[2] as f32,
                0.0,
            ],
            mie: [
                p.mie_m_inv as f32,
                p.mie_anisotropy as f32,
                p.ozone_half_width_m as f32,
                0.0,
            ],
            ozone: [
                p.ozone_rgb_m_inv[0] as f32,
                p.ozone_rgb_m_inv[1] as f32,
                p.ozone_rgb_m_inv[2] as f32,
                p.ozone_peak_height_m as f32,
            ],
            // Preserve the globe's existing map-readable night exposure.
            config: [radius, f32::from(standard_quality), 1.0, 0.018],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&value));
    }

    pub(super) fn resize(&mut self, device: &wgpu::Device, depth: &wgpu::TextureView) {
        self.depth_bind_group = pipeline::depth_group(device, &self.depth_layout, depth);
    }
}
