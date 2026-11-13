//! GPU texture and bind group for the bundled Milky Way sky background.
mod bolid_dynamics;
mod bolid_history;
mod bolid_model;
pub use bolid_dynamics::BolidFamily;
pub use bolid_history::EvolutionDiagnostics;
pub use bolid_model::BolidEvolution;

const WIDTH: u32 = 2048;
const HEIGHT: u32 = 1024;
const PIXELS: &[u8] = include_bytes!("../../assets/data/sky/milky-way-v1.rgb9e5");

pub const MAX_BOLIDS: usize = 8;

#[repr(C, align(16))]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BolidGpuEvent {
    pub start_age: [f32; 4],
    pub end_duration: [f32; 4],
    pub style: [f32; 4],
    pub head_power: [f32; 4],
    pub light_position_power: [f32; 4],
    pub emission: [f32; 4],
    pub bounds: [f32; 4],
    pub fragments: [[f32; 4]; 19],
    /// Historical wake endpoint xyz and radiant power at that endpoint.
    pub tails: [[f32; 4]; 20],
    /// Width, temperature, breakup age, and grain flag + fractional wake age.
    pub optics: [[f32; 4]; 20],
    pub trail: [[f32; 4]; 24],
    pub trail_meta: [[f32; 4]; 24],
}

#[repr(C, align(16))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BolidGpuFrame {
    pub events: [BolidGpuEvent; MAX_BOLIDS],
    pub count: [u32; 4],
}

const _: () = {
    assert!(std::mem::size_of::<BolidGpuEvent>() == 1824);
    assert!(std::mem::offset_of!(BolidGpuEvent, fragments) == 112);
    assert!(std::mem::offset_of!(BolidGpuEvent, tails) == 416);
    assert!(std::mem::offset_of!(BolidGpuEvent, optics) == 736);
    assert!(std::mem::offset_of!(BolidGpuEvent, trail) == 1056);
    assert!(std::mem::offset_of!(BolidGpuEvent, trail_meta) == 1440);
    assert!(std::mem::size_of::<BolidGpuFrame>() == 14608);
};

impl Default for BolidGpuFrame {
    fn default() -> Self {
        Self {
            events: [BolidGpuEvent::default(); MAX_BOLIDS],
            count: [0; 4],
        }
    }
}

pub struct MilkyWayGpu {
    pub meteors: crate::meteors_gpu::MeteorGpu,
    _texture: wgpu::Texture,
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    bolids: wgpu::Buffer,
    has_dust: bool,
    active: bool,
    pub(crate) layer: Option<crate::bolid_layer::BolidLayer>,
    pub occlusion_layout: wgpu::BindGroupLayout,
    pub occlusion: Option<wgpu::BindGroup>,
}

impl MilkyWayGpu {
    pub const BYTES: u64 = (WIDTH as u64) * (HEIGHT as u64) * 4;
    pub const BOLID_BYTES: u64 = std::mem::size_of::<BolidGpuFrame>() as u64;

    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, camera: &wgpu::BindGroupLayout) -> Self {
        assert_eq!(PIXELS.len() as u64, Self::BYTES);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("NASA SVS Milky Way background"),
            size: wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgb9e5Ufloat,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            PIXELS,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: Some(HEIGHT),
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Milky Way celestial sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bolids = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bounded bolid sky events"),
            size: Self::BOLID_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Milky Way texture layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Milky Way texture bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: bolids.as_entire_binding(),
                },
            ],
        });
        let occlusion_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bolid finite-distance occlusion"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Depth,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        Self {
            meteors: crate::meteors_gpu::MeteorGpu::new(device, camera, &occlusion_layout),
            _texture: texture,
            layout,
            bind_group,
            bolids,
            has_dust: false,
            active: false,
            layer: None,
            occlusion_layout,
            occlusion: None,
        }
    }

    pub fn has_dust(&self) -> bool {
        self.has_dust
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn any_emission(&self) -> bool {
        self.active || self.meteors.count > 0
    }

    pub fn layer_bytes(&self) -> u64 {
        self.layer.as_ref().map_or(0, |layer| {
            u64::from(layer.size[0])
                * u64::from(layer.size[1])
                * crate::bolid_layer::BYTES_PER_PIXEL
                + 16
        })
    }

    pub fn set_transmission_diagnostic(&self, queue: &wgpu::Queue, enabled: bool) -> bool {
        let Some(layer) = &self.layer else {
            return false;
        };
        queue.write_buffer(
            &layer.diagnostic,
            0,
            bytemuck::cast_slice(&[u32::from(enabled), 0, 0, 0]),
        );
        true
    }

    pub fn set_depth(&mut self, device: &wgpu::Device, depth: &wgpu::TextureView, size: [u32; 2]) {
        self.layer = Some(crate::bolid_layer::BolidLayer::new(device, size));
        self.occlusion = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bolid scene depth"),
            layout: &self.occlusion_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(depth),
            }],
        }));
    }

    pub fn upload_bolids(&mut self, queue: &wgpu::Queue, frame: &BolidGpuFrame) {
        self.active = frame.count[0] > 0;
        self.has_dust = frame
            .events
            .iter()
            .take(frame.count[0] as usize)
            .any(|event| event.style[3] > 0.0 && event.start_age[3] > 0.0);
        let mut upload = *frame;
        if let Some(layer) = &self.layer {
            upload.count[1] = layer.size[0];
            upload.count[2] = layer.size[1];
        }
        queue.write_buffer(&self.bolids, 0, bytemuck::bytes_of(&upload));
    }
}
