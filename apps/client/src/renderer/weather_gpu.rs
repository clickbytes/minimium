mod atmosphere;
pub(super) mod capabilities;
mod pipeline;
mod targets;
mod timing;
mod upload;
use crate::environment::weather::{LightningEvent, WeatherSample, ATLAS_LAYERS};
use bytemuck::{Pod, Zeroable};
use minimium_weather::AtmosphereProfile;
use pipeline::{depth_entry, depth_group, pipeline};
use wgpu::util::DeviceExt;

#[repr(C, align(16))]
#[derive(Clone, Copy, Default, Pod, Zeroable)]
struct WeatherUniform {
    cover_rain: [f32; 4],
    wind: [f32; 4],
    blend: [f32; 4],
    origin: [f32; 4],
    lightning: [f32; 4],
    event: [f32; 4],
    shelter: [f32; 4],
    region: [f32; 4],
    animation: [f32; 4],
    cloud_region: [f32; 4],
    presentation: [f32; 4],
    atmosphere_radii: [f32; 4],
    atmosphere_rayleigh: [f32; 4],
    atmosphere_mie: [f32; 4],
    atmosphere_ozone: [f32; 4],
}

const _: () = {
    assert!(std::mem::size_of::<WeatherUniform>() == 15 * 16);
    assert!(std::mem::align_of::<WeatherUniform>() == 16);
};

pub(super) struct WeatherFrame {
    pub(super) rain_ceiling_m: f32,
    pub(super) readable_globe: bool,
    pub(super) sample: WeatherSample,
    pub(super) blend: f32,
    pub(super) region: [f32; 4],
    pub(super) overview: bool,
    pub(super) seed: u128,
    pub(super) radius: f32,
    pub(super) origin: [f32; 3],
    pub(super) utc_ms: Option<i64>,
    pub(super) lightning: Option<LightningEvent>,
    pub(super) standard_quality: bool,
    pub(super) shelter_mask: [u16; 4],
}

pub(super) struct WeatherGpu {
    pub(super) capabilities: capabilities::CloudCapabilities,
    pub(super) timing: timing::CloudTiming,
    pub(super) cloud_settings: super::cloud_settings::CloudRenderSettings,
    pub(super) cloud_effective: super::cloud_settings::EffectiveCloudSettings,
    atlas: wgpu::Texture,
    atmosphere_profile: AtmosphereProfile,
    depth_layout: wgpu::BindGroupLayout,
    pub(super) depth_bind_group: wgpu::BindGroup,
    pub(super) globe_layout: wgpu::BindGroupLayout,
    pub(super) globe_bind_group: wgpu::BindGroup,
    atlas_revision: u64,
    published_revision: u64,
    uniform: wgpu::Buffer,
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) bind_group: wgpu::BindGroup,
    pub(super) clouds: wgpu::RenderPipeline,
    pub(super) cloud_aggregates: wgpu::RenderPipeline,
    pub(super) instances: super::cloud_instances::CloudInstances,
    pub(super) fog: wgpu::RenderPipeline,
    pub(super) atmosphere: wgpu::RenderPipeline,
    pub(super) cloud_composite: wgpu::RenderPipeline,
    pub(super) cloud_targets: targets::CloudTargets,
    cloud_layout: wgpu::BindGroupLayout,
    pub(super) cloud_bytes: u64,
    pub(super) rain: wgpu::RenderPipeline,
    pub(super) bolt: wgpu::RenderPipeline,
    pub(super) rain_mm_h: f32,
    pub(super) rain_particles: u32,
    pub(super) bolt_active: bool,
}

impl WeatherGpu {
    pub(super) const BYTES: u64 = 512 * 256 * 4 * ATLAS_LAYERS as u64
        + super::cloud_instances::CloudInstances::BYTES
        + std::mem::size_of::<WeatherUniform>() as u64
        + atmosphere::BYTES
        + timing::CloudTiming::BYTES;

    pub(super) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        size: winit::dpi::PhysicalSize<u32>,
        depth: &wgpu::TextureView,
        capabilities: capabilities::CloudCapabilities,
    ) -> Self {
        let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("weather uniform"),
            contents: bytemuck::bytes_of(&WeatherUniform::default()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let atlas = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Earth weather atlas"),
            size: wgpu::Extent3d {
                width: 512,
                height: 256,
                depth_or_array_layers: ATLAS_LAYERS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
        let atmosphere_profile = AtmosphereProfile::earth();
        atmosphere_profile.validate().expect("bundled Earth atmosphere must be valid");
        let atmosphere_view = atmosphere::create(device, queue, atmosphere_profile);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("weather atlas sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let instances =
            super::cloud_instances::CloudInstances::new(device, capabilities.optical_scale());
        let cloud_settings = super::cloud_settings::CloudRenderSettings::default();
        let cloud_effective =
            cloud_settings.effective(true, &device.limits(), capabilities.precise);
        let shadow_view = instances
            .shadows
            .create_view(&wgpu::TextureViewDescriptor::default());
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("weather layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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
                pipeline::shadow_entry(),
                atmosphere::entry(),
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
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&atmosphere_view),
                },
            ],
        });
        let globe_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globe weather layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
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
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                pipeline::shadow_entry(),
                atmosphere::entry(),
            ],
        });
        let globe_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globe weather bind group"),
            layout: &globe_layout,
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
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&shadow_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&atmosphere_view),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("weather pipeline layout"),
            bind_group_layouts: &[Some(frame_layout), Some(&layout)],
            immediate_size: 0,
        });
        let depth_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("cloud scene depth layout"),
            entries: &[depth_entry(0)],
        });
        let depth_bind_group = depth_group(device, &depth_layout, depth);
        let cloud_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("geometric cloud pipeline layout"),
                bind_group_layouts: &[Some(frame_layout), Some(&layout), Some(&depth_layout)],
                immediate_size: 0,
            });
        let instance_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("analytic cloud pipeline layout"),
                bind_group_layouts: &[
                    Some(frame_layout),
                    Some(&layout),
                    Some(&depth_layout),
                    Some(&instances.layout),
                ],
                immediate_size: 0,
            });
        let clouds = pipeline::cloud_pipeline(
            device,
            &instance_pipeline_layout,
            false,
            capabilities.format,
        );
        let cloud_aggregates =
            pipeline::cloud_pipeline(device, &instance_pipeline_layout, true, capabilities.format);
        let atmosphere = pipeline(
            device,
            &cloud_pipeline_layout,
            format,
            "orbital-atmosphere",
            super::shader_source::atmosphere(),
            ("vs_main", "fs_atmosphere"),
            None,
        );
        let fog = pipeline(
            device,
            &cloud_pipeline_layout,
            format,
            "weather-fog",
            super::shader_source::weather_fog(),
            ("vs_main", "fs_fog"),
            None,
        );
        let rain = pipeline(
            device,
            &pipeline_layout,
            format,
            "weather-rain",
            super::shader_source::rain(),
            ("vs_rain", "fs_rain"),
            Some(wgpu::CompareFunction::LessEqual),
        );
        let bolt = pipeline(
            device,
            &pipeline_layout,
            format,
            "weather-bolt",
            super::shader_source::bolt(),
            ("vs_bolt", "fs_bolt"),
            Some(wgpu::CompareFunction::LessEqual),
        );
        let cloud_layout = targets::layout(device);
        let cloud_targets = targets::CloudTargets::new(
            device,
            &cloud_layout,
            size,
            depth,
            cloud_effective.scale,
            capabilities.format,
        );
        let composite_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("cloud composite pipeline layout"),
            bind_group_layouts: &[Some(frame_layout), Some(&layout), Some(&cloud_layout)],
            immediate_size: 0,
        });
        let cloud_composite = pipeline(
            device,
            &composite_layout,
            format,
            "cloud-composite",
            super::shader_source::cloud_composite(),
            ("vs_main", "fs_main"),
            None,
        );
        Self {
            cloud_settings,
            cloud_effective,
            cloud_aggregates,
            atlas,
            atmosphere_profile,
            depth_layout,
            depth_bind_group,
            globe_layout,
            globe_bind_group,
            atlas_revision: u64::MAX,
            published_revision: u64::MAX,
            uniform,
            layout,
            bind_group,
            clouds,
            instances,
            fog,
            cloud_composite,
            cloud_targets,
            cloud_layout,
            atmosphere,
            cloud_bytes: targets::bytes(size, cloud_effective.scale, capabilities.format),
            capabilities,
            timing: timing::CloudTiming::new(device, queue),
            rain,
            bolt,
            rain_mm_h: 0.0,
            rain_particles: 0,
            bolt_active: false,
        }
    }

    pub(super) fn stage_atlas(
        &mut self,
        pixels: [&[u8]; ATLAS_LAYERS],
        revision: u64,
        region: [f32; 4],
        start_seconds: Option<f64>,
    ) {
        if self.atlas_revision == revision {
            return;
        }
        self.atlas_revision = revision;
        self.instances
            .set_atlas(pixels, revision, region, start_seconds);
    }

    pub(super) fn publish_atlas(&mut self, queue: &wgpu::Queue) {
        self.instances
            .publish_atlas(queue, &self.atlas, &mut self.published_revision);
    }

    pub(super) fn resize(
        &mut self,
        device: &wgpu::Device,
        size: winit::dpi::PhysicalSize<u32>,
        depth: &wgpu::TextureView,
    ) {
        self.cloud_targets = targets::CloudTargets::new(
            device,
            &self.cloud_layout,
            size,
            depth,
            self.cloud_effective.scale,
            self.capabilities.format,
        );
        self.depth_bind_group = depth_group(device, &self.depth_layout, depth);
        self.cloud_bytes =
            targets::bytes(size, self.cloud_effective.scale, self.capabilities.format);
    }
}
