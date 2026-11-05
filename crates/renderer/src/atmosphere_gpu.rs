//! GPU owner for clear-air atmosphere bindings, LUT and draw pipeline.

use crate::AtmosphereUniform;
use wgpu::util::DeviceExt;

pub struct AtmosphereGpu {
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
    pub depth_bind_group: wgpu::BindGroup,
    depth_layout: wgpu::BindGroupLayout,
    uniform: wgpu::Buffer,
}

impl AtmosphereGpu {
    pub const BYTES: u64 =
        crate::atmosphere_table::BYTES + std::mem::size_of::<AtmosphereUniform>() as u64;

    pub fn new(
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
        let table = crate::atmosphere_table::create(device, queue, crate::atmosphere::EARTH);
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
                crate::atmosphere_table::entry(),
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
            entries: &[crate::atmosphere_pipeline::depth_entry()],
        });
        let depth_bind_group =
            crate::atmosphere_pipeline::depth_group(device, &depth_layout, depth);
        let pipeline = crate::atmosphere_pipeline::create(
            device,
            frame_layout,
            &layout,
            &depth_layout,
            format,
        );
        Self {
            layout,
            bind_group,
            pipeline,
            depth_bind_group,
            depth_layout,
            uniform,
        }
    }

    pub fn upload(&self, queue: &wgpu::Queue, radius: f32, standard_quality: bool) {
        let p = crate::atmosphere::EARTH;
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

    pub fn resize(&mut self, device: &wgpu::Device, depth: &wgpu::TextureView) {
        self.depth_bind_group =
            crate::atmosphere_pipeline::depth_group(device, &self.depth_layout, depth);
    }

    /// Encodes the clear-air limb pass when its globe view is active.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        scene: &wgpu::TextureView,
        camera: &wgpu::BindGroup,
        enabled: bool,
    ) -> u32 {
        if !enabled {
            return 0;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("orbital-atmosphere-limb"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: scene,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &self.bind_group, &[]);
        pass.set_bind_group(2, &self.depth_bind_group, &[]);
        pass.draw(0..3, 0..1);
        1
    }
}
