//! Validated material texture arrays and terrain pipeline.
use crate::terrain_geometry::TerrainVertex;
use minimium_assets::{
    ktx2::Texture,
    terrain_material::{MaterialManifest, MAX_PACKAGE_BYTES},
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TerrainUniform {
    pub scales_a: [f32; 4],
    pub scales_b_blend: [f32; 4],
    /// Hour interpolation, history validity, planet atlas validity, reserved.
    pub cover: [f32; 4],
    /// Debug mode, two folded words covering all 128 seed bits, reserved.
    pub debug: [u32; 4],
    pub cover_center: [f32; 4],
    pub cover_east: [f32; 4],
    pub cover_north: [f32; 4],
    pub eye_relative: [f32; 4],
    /// Seasonal phase, scenario temperature/moisture, relief influence on climate.
    pub climate: [f32; 4],
    pub mean_albedo: [[f32; 4]; 6],
}

pub use crate::terrain_debug::TerrainDebug;

pub struct TerrainGpu {
    pub valid: bool,
    textures: Vec<wgpu::Texture>,
    pub pipeline: wgpu::RenderPipeline,
    pub ocean_pipeline: wgpu::RenderPipeline,
    pub bind_group: wgpu::BindGroup,
    pub uniform_buffer: wgpu::Buffer,
    pub uniform: TerrainUniform,
    pub bytes: u64,
    history: crate::terrain_cover_gpu::TerrainCoverGpu,
}

impl TerrainGpu {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: &wgpu::BindGroupLayout,
        shadow: &wgpu::BindGroupLayout,
        color: wgpu::TextureFormat,
        manifest: &MaterialManifest,
        variant: usize,
        maps: &[Vec<u8>; 3],
    ) -> Result<Self, String> {
        manifest.validate()?;
        let selected = manifest
            .variants
            .get(variant)
            .ok_or("invalid material variant")?;
        let formats = if selected.compressed {
            [132, 141, 137]
        } else {
            [43, 37, 37]
        };
        // Complete validation precedes every allocation.
        let mut parsed = Vec::new();
        let mut bytes = 0;
        for (index, data) in maps.iter().enumerate() {
            let file = &selected.maps[index];
            if data.len() as u64 != file.bytes
                || minimium_assets::terrain_material::digest(data) != file.sha256
            {
                return Err("material file hash mismatch".into());
            }
            let texture = Texture::parse(data, selected.edge, formats[index])?;
            bytes += texture
                .levels
                .iter()
                .skip(base_mip(&texture))
                .map(|range| range.len() as u64)
                .sum::<u64>();
            parsed.push(texture);
        }
        if bytes > MAX_PACKAGE_BYTES
            || selected.edge > device.limits().max_texture_dimension_2d
            || device.limits().max_texture_array_layers < 6
            || (selected.compressed
                && !device
                    .features()
                    .contains(wgpu::Features::TEXTURE_COMPRESSION_BC))
        {
            return Err("terrain materials exceed GPU capabilities or budget".into());
        }
        let textures: Vec<_> = parsed
            .iter()
            .map(|texture| upload(device, queue, texture))
            .collect();
        let views: Vec<_> = textures
            .iter()
            .map(|texture| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    dimension: Some(wgpu::TextureViewDimension::D2Array),
                    ..Default::default()
                })
            })
            .collect();
        let mut entries: Vec<_> = (0..3)
            .map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            })
            .collect();
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 3,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 4,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(
                    std::mem::size_of::<TerrainUniform>() as u64
                ),
            },
            count: None,
        });
        for binding in [5, 6] {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2Array,
                    multisampled: false,
                },
                count: None,
            });
        }
        let history = crate::terrain_cover_gpu::TerrainCoverGpu::new(device);
        let cover_view = history.local.create_view(&Default::default());
        let atlas_view = history.global.create_view(&Default::default());
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain-materials"),
            entries: &entries,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("terrain-repeat"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 4,
            ..Default::default()
        });
        let uniform = TerrainUniform {
            scales_a: manifest.meters_per_tile[..4].try_into().unwrap(),
            scales_b_blend: [
                manifest.meters_per_tile[4],
                manifest.meters_per_tile[5],
                manifest.height_blend,
                0.0,
            ],
            cover: [0.0; 4],
            debug: [0; 4],
            cover_center: [0.0; 4],
            cover_east: [0.0; 4],
            cover_north: [0.0; 4],
            eye_relative: [0.0; 4],
            climate: [0.0; 4],
            mean_albedo: crate::terrain_palette::colors(&parsed[0]),
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("terrain-settings"),
            contents: bytemuck::bytes_of(&uniform),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-materials"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&views[0]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&views[1]),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&views[2]),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&cover_view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain-pipeline"),
            bind_group_layouts: &[Some(frame), Some(&layout), Some(shadow)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl(crate::shader_source::terrain().into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_terrain"),
                compilation_options: Default::default(),
                buffers: &[Some(TerrainVertex::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_terrain"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        bytes += crate::terrain_cover_gpu::TerrainCoverGpu::BYTES;
        let ocean_pipeline = crate::ocean_pipeline::create(device, &pipeline_layout, color);
        Ok(Self {
            valid: true,
            textures,
            pipeline,
            ocean_pipeline,
            bind_group,
            uniform_buffer,
            uniform,
            bytes,
            history,
        })
    }

    pub fn update(&self, queue: &wgpu::Queue) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&self.uniform));
    }

    /// Release the previous package before allocating a replacement under the GPU budget.
    pub fn release_materials(&mut self) {
        self.valid = false;
        for texture in &self.textures {
            texture.destroy();
        }
    }

    pub fn upload_cover(&mut self, queue: &wgpu::Queue, grid: &crate::surface_cover::CoverGrid) {
        self.uniform.cover[2] = f32::from(self.history.upload(queue, grid));
        self.uniform.cover_center = grid.region;
        self.update(queue);
    }

    pub fn clear_cover(&mut self) {
        self.history.clear();
        self.uniform.cover = [0.0; 4];
        self.uniform.cover_center = [0.0; 4];
    }

    pub fn set_cover_time(&mut self, utc: f64) {
        let hours = self.history.start_utc.map(|start| (utc - start) / 3600.0);
        self.uniform.cover[0] = hours.unwrap_or(0.0).clamp(0.0, 1.0) as f32;
        // Keep the shared endpoint while the next worker result is in flight.
        // Stale data fades over a world hour, rather than flashing to fallback.
        self.uniform.cover[1] = hours.filter(|h| *h >= 0.0).map_or(0.0, |h| {
            let t = (h - 1.0).clamp(0.0, 1.0) as f32;
            1.0 - t * t * (3.0 - 2.0 * t)
        });
    }
}

const _: () = {
    assert!(std::mem::size_of::<TerrainUniform>() == 240);
    assert!(std::mem::offset_of!(TerrainUniform, mean_albedo) == 144);
};

fn upload(device: &wgpu::Device, queue: &wgpu::Queue, source: &Texture<'_>) -> wgpu::Texture {
    let first_mip = base_mip(source);
    let resident_edge = source.edge >> first_mip;
    let (format, block, block_bytes) = match source.vk_format {
        132 => (wgpu::TextureFormat::Bc1RgbaUnormSrgb, 4, 8),
        137 => (wgpu::TextureFormat::Bc3RgbaUnorm, 4, 16),
        141 => (wgpu::TextureFormat::Bc5RgUnorm, 4, 16),
        43 => (wgpu::TextureFormat::Rgba8UnormSrgb, 1, 4),
        _ => (wgpu::TextureFormat::Rgba8Unorm, 1, 4),
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("terrain-array"),
        size: wgpu::Extent3d {
            width: resident_edge,
            height: resident_edge,
            depth_or_array_layers: 6,
        },
        mip_level_count: (source.levels.len() - first_mip) as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (mip, range) in source.levels.iter().enumerate().skip(first_mip) {
        let edge = (source.edge >> mip).max(1);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: (mip - first_mip) as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &source.bytes[range.clone()],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(edge.div_ceil(block) * block_bytes),
                rows_per_image: Some(edge.div_ceil(block)),
            },
            wgpu::Extent3d {
                width: edge.max(block),
                height: edge.max(block),
                depth_or_array_layers: 6,
            },
        );
    }
    texture
}

fn base_mip(source: &Texture<'_>) -> usize {
    // Existing packages retain their full source detail; default residency is 2K.
    usize::from(source.edge > 2048)
}
