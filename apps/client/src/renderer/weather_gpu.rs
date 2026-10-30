use crate::environment::weather::WeatherSample;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C, align(16))]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct WeatherUniform {
    cover_rain: [f32; 4],
    wind: [f32; 4],
}

pub(super) struct WeatherGpu {
    _noise: wgpu::Texture,
    uniform: wgpu::Buffer,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) clouds: wgpu::RenderPipeline,
    pub(super) rain: wgpu::RenderPipeline,
    pub(super) cloud_cover: f32,
    pub(super) rain_mm_h: f32,
}

impl WeatherGpu {
    pub(super) const BYTES: u64 = 256 * 256 + std::mem::size_of::<WeatherUniform>() as u64;

    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> Self {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("weather uniform"),
            contents: bytemuck::bytes_of(&WeatherUniform::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let noise = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tileable cloud noise"),
            size: wgpu::Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &noise,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &tileable_noise(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(256),
            },
            wgpu::Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
        );
        let view = noise.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("cloud noise sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("weather layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("weather bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weather pipeline layout"),
            bind_group_layouts: &[Some(frame_layout), Some(&layout)],
            immediate_size: 0,
        });
        let clouds = pipeline(
            device,
            &pipeline_layout,
            format,
            "weather-clouds",
            super::shader_source::clouds(),
        );
        let rain = pipeline(
            device,
            &pipeline_layout,
            format,
            "weather-rain",
            super::shader_source::rain(),
        );
        Self {
            _noise: noise,
            uniform,
            bind_group,
            clouds,
            rain,
            cloud_cover: 0.0,
            rain_mm_h: 0.0,
        }
    }

    pub(super) fn upload(&mut self, queue: &wgpu::Queue, sample: WeatherSample) {
        let value = WeatherUniform {
            cover_rain: [sample.low, sample.middle, sample.high, sample.rain_mm_h],
            wind: [sample.wind_east_m_s, sample.wind_north_m_s, 0.0, 0.0],
        };
        queue.write_buffer(&self.uniform, 0, bytemuck::bytes_of(&value));
        self.cloud_cover = sample.low.max(sample.middle).max(sample.high);
        self.rain_mm_h = sample.rain_mm_h;
    }
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    label: &str,
    source: String,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::Always),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn tileable_noise() -> Vec<u8> {
    let mut pixels = vec![0; 256 * 256];
    for y in 0..256 {
        for x in 0..256 {
            let mut value = 0.0;
            let mut weight = 0.0;
            for (cells, amplitude) in [(8, 0.50), (16, 0.30), (32, 0.20)] {
                let px = x as f32 * cells as f32 / 256.0;
                let py = y as f32 * cells as f32 / 256.0;
                let ix = px.floor() as i32;
                let iy = py.floor() as i32;
                let sx = smooth(px.fract());
                let sy = smooth(py.fract());
                let a = blend(hash(ix, iy, cells), hash(ix + 1, iy, cells), sx);
                let b = blend(hash(ix, iy + 1, cells), hash(ix + 1, iy + 1, cells), sx);
                value += blend(a, b, sy) * amplitude;
                weight += amplitude;
            }
            pixels[y * 256 + x] = ((value / weight) * 255.0) as u8;
        }
    }
    let mut counts = [0_u32; 256];
    for value in &pixels {
        counts[*value as usize] += 1;
    }
    let mut cumulative = 0_u32;
    let mut percentile = [0_u8; 256];
    for (index, count) in counts.into_iter().enumerate() {
        cumulative += count;
        percentile[index] = ((cumulative as f32 / pixels.len() as f32) * 255.0) as u8;
    }
    for value in &mut pixels {
        *value = percentile[*value as usize];
    }
    pixels
}

fn hash(x: i32, y: i32, period: i32) -> f32 {
    let x = x.rem_euclid(period) as u32;
    let y = y.rem_euclid(period) as u32;
    let mut value = x.wrapping_mul(0x9e37_79b9) ^ y.wrapping_mul(0x85eb_ca6b);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    (value as f32) / (u32::MAX as f32)
}

fn smooth(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn blend(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
