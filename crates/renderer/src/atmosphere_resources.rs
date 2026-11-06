//! Bounded atmosphere textures shared by all scene pipelines through group zero.
use crate::AtmosphereUniform;
use wgpu::util::DeviceExt;

pub struct AtmosphereResources {
    pub uniform: wgpu::Buffer,
    pub physical: bool,
    pub optical: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
    pub textures: Vec<wgpu::Texture>,
    pub views: Vec<wgpu::TextureView>,
    pub staging: Vec<wgpu::Texture>,
    pub stages: Vec<wgpu::TextureView>,
    pub fog: wgpu::Texture,
    pub fog_view: wgpu::TextureView,
}

impl AtmosphereResources {
    pub const SIZES: [[u32; 3]; 5] = [
        [192, 108, 1],
        [192, 108, 1],
        [32, 32, 32],
        [32, 32, 32],
        [32, 32, 1],
    ];
    pub const BYTES: u64 = crate::atmosphere_table::BYTES
        + 256
        + 9 * 9 * 2 * 8
        + 2 * (192 * 108 * 16 + 32 * 32 * 32 * 16 + 32 * 32 * 8);

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, physical: bool) -> Self {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("atmosphere view and optics"),
            contents: bytemuck::bytes_of(&AtmosphereUniform::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let optical = crate::atmosphere_table::create(device, queue, crate::atmosphere::EARTH);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atmosphere linear clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let make = |size: [u32; 3], usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bounded atmosphere LUT"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: size[2],
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: if size[2] > 1 {
                    wgpu::TextureDimension::D3
                } else {
                    wgpu::TextureDimension::D2
                },
                format: wgpu::TextureFormat::Rgba16Float,
                usage,
                view_formats: &[],
            })
        };
        let textures: Vec<_> = Self::SIZES
            .map(|s| {
                make(
                    s,
                    wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                )
            })
            .into();
        let staging: Vec<_> = if physical {
            Self::SIZES
                .map(|s| {
                    make(
                        s,
                        wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
                    )
                })
                .into()
        } else {
            Vec::new()
        };
        let views = textures
            .iter()
            .map(|t| t.create_view(&Default::default()))
            .collect();
        let stages = staging
            .iter()
            .map(|t| t.create_view(&Default::default()))
            .collect();
        let fog = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("regional fog endpoints"),
            size: wgpu::Extent3d {
                width: 9,
                height: 9,
                depth_or_array_layers: 2,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let fog_view = fog.create_view(&Default::default());
        Self {
            physical,
            uniform,
            optical,
            sampler,
            textures,
            views,
            staging,
            stages,
            fog,
            fog_view,
        }
    }

    pub fn entries(&self) -> Vec<wgpu::BindGroupEntry<'_>> {
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 2,
                resource: self.uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(&self.optical),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(&self.sampler),
            },
        ];
        entries.extend(
            self.views
                .iter()
                .enumerate()
                .map(|(i, v)| wgpu::BindGroupEntry {
                    binding: 5 + i as u32,
                    resource: wgpu::BindingResource::TextureView(v),
                }),
        );
        entries.push(wgpu::BindGroupEntry {
            binding: 10,
            resource: wgpu::BindingResource::TextureView(&self.fog_view),
        });
        entries
    }

    pub fn layout_entries() -> Vec<wgpu::BindGroupLayoutEntry> {
        let visibility = wgpu::ShaderStages::FRAGMENT | wgpu::ShaderStages::COMPUTE;
        let mut entries = vec![
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT | wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(256),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        for binding in [3, 5, 6, 7, 8, 9, 10] {
            entries.push(wgpu::BindGroupLayoutEntry {
                binding,
                visibility,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: if binding == 10 {
                        wgpu::TextureViewDimension::D2Array
                    } else if binding == 7 || binding == 8 {
                        wgpu::TextureViewDimension::D3
                    } else {
                        wgpu::TextureViewDimension::D2
                    },
                    multisampled: false,
                },
                count: None,
            });
        }
        entries
    }

    pub fn bytes(&self) -> u64 {
        crate::atmosphere_table::BYTES
            + self.uniform.size()
            + 9 * 9 * 2 * 8
            + self
                .textures
                .iter()
                .chain(self.staging.iter())
                .map(|t| {
                    let s = t.size();
                    s.width as u64 * s.height as u64 * s.depth_or_array_layers as u64 * 8
                })
                .sum::<u64>()
    }

    pub fn publish(&self, encoder: &mut wgpu::CommandEncoder, range: std::ops::Range<usize>) {
        for i in range {
            encoder.copy_texture_to_texture(
                self.staging[i].as_image_copy(),
                self.textures[i].as_image_copy(),
                self.textures[i].size(),
            );
        }
    }
}
