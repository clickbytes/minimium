use crate::{
    geometry::Vertex,
    shadow_target::{create_target_checked, uniform_layout_entry, DEFAULT_RESOLUTION},
    shadow_uniform::ShadowUniform,
    ShadowSettings,
};
use bytemuck::Zeroable;
use wgpu::util::DeviceExt;

pub struct ShadowGpu {
    pub settings: ShadowSettings,
    pub fallback_reason: Option<&'static str>,
    format: wgpu::TextureFormat,
    pub depth_pipeline: wgpu::RenderPipeline,
    pub depth_view: wgpu::TextureView,
    pub uniform_bind_group: wgpu::BindGroup,
    pub sampling_bind_group: wgpu::BindGroup,
    pub empty_bind_group: wgpu::BindGroup,
    pub empty_layout: wgpu::BindGroupLayout,
    pub group2_layout: wgpu::BindGroupLayout,
    pub uniform_buffer: wgpu::Buffer,
    map_casters: Option<wgpu::Buffer>,
    map_caster_count: u32,
    player_casters: Option<wgpu::Buffer>,
    player_caster_count: u32,
    _texture: wgpu::Texture,
    sampler: wgpu::Sampler,
    gpu_bytes: u64,
}

impl ShadowGpu {
    pub fn new(
        device: &wgpu::Device,
        settings: ShadowSettings,
        format: wgpu::TextureFormat,
    ) -> Result<Self, &'static str> {
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shadow-depth-uniform-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(128),
                },
                count: None,
            }],
        });
        let group2_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-shadow-sampling-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                uniform_layout_entry(2, wgpu::ShaderStages::VERTEX_FRAGMENT),
            ],
        });
        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-material-reserved-layout"),
            entries: &[],
        });
        let empty_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world-material-empty-bind-group"),
            layout: &empty_layout,
            entries: &[],
        });
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("world-shadow-uniform"),
            contents: bytemuck::bytes_of(&ShadowUniform::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shadow-depth-uniform-bind-group"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-comparison-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("local-shadow-depth-shader"),
            source: wgpu::ShaderSource::Wgsl(crate::shaders::SHADOW_WGSL.into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("local-shadow-depth-pipeline-layout"),
            bind_group_layouts: &[Some(&uniform_layout)],
            immediate_size: 0,
        });
        let depth_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("local-shadow-depth-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_shadow"),
                compilation_options: Default::default(),
                buffers: &[Some(Vertex::layout())],
            },
            fragment: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let (settings, target, fallback_reason) = match create_target_checked(
            device,
            &group2_layout,
            &uniform_buffer,
            &sampler,
            settings,
            format,
        ) {
            Ok(target) => (settings, target, None),
            Err(_) if settings.enabled => {
                let fallback = ShadowSettings::for_quality(crate::quality::SkyQuality::Low);
                let target = create_target_checked(
                    device,
                    &group2_layout,
                    &uniform_buffer,
                    &sampler,
                    fallback,
                    format,
                )
                .map_err(|_| "fallback_shadow_texture_creation_failed")?;
                (
                    fallback,
                    target,
                    Some("standard_shadow_texture_allocation_refused"),
                )
            }
            Err(reason) => return Err(reason),
        };
        let (texture, depth_view, sampling_bind_group, gpu_bytes) = target;
        Ok(Self {
            settings,
            fallback_reason,
            format,
            depth_pipeline,
            depth_view,
            uniform_bind_group,
            sampling_bind_group,
            empty_bind_group,
            empty_layout,
            group2_layout,
            uniform_buffer,
            map_casters: None,
            map_caster_count: 0,
            player_casters: None,
            player_caster_count: 0,
            _texture: texture,
            sampler,
            gpu_bytes,
        })
    }

    /// Encodes the depth pass for the caster buffers selected by the world adapter.
    pub fn encode_depth_pass<'a>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        casters: impl IntoIterator<Item = (&'a wgpu::Buffer, u32)>,
        enabled: bool,
    ) -> u32 {
        if !enabled {
            return 0;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("local-shadow-depth-pass"),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(&self.depth_pipeline);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        let mut draws = 0;
        for (buffer, vertex_count) in casters {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.draw(0..vertex_count, 0..1);
            draws += 1;
        }
        draws
    }

    pub fn upload_map_casters(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[Vertex],
        capacity_bytes: u64,
    ) -> Result<u64, ()> {
        let bytes = std::mem::size_of_val(vertices) as u64;
        if bytes > capacity_bytes {
            self.map_caster_count = 0;
            return Err(());
        }
        self.map_caster_count = vertices.len() as u32;
        if vertices.is_empty() {
            return Ok(0);
        }
        let mut allocated = 0;
        if self.map_casters.is_none() {
            self.map_casters = Some(create_caster_buffer(
                device,
                "local-shadow-map-casters",
                capacity_bytes,
            ));
            allocated = capacity_bytes;
        }
        if let Some(buffer) = &self.map_casters {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(vertices));
        }
        Ok(allocated)
    }

    pub fn map_casters(&self) -> Option<(&wgpu::Buffer, u32)> {
        (self.map_caster_count > 0)
            .then(|| {
                self.map_casters
                    .as_ref()
                    .map(|buffer| (buffer, self.map_caster_count))
            })
            .flatten()
    }

    pub fn map_caster_buffer_bytes(&self) -> u64 {
        self.map_casters.as_ref().map_or(0, wgpu::Buffer::size)
    }

    pub fn release_map_casters(&mut self) -> u64 {
        self.map_caster_count = 0;
        self.map_casters.take().map_or(0, |buffer| buffer.size())
    }

    pub fn clear_map_casters(&mut self) {
        self.map_caster_count = 0;
    }

    pub fn upload_player_casters(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        vertices: &[Vertex],
        capacity_bytes: u64,
    ) -> Result<u64, ()> {
        let bytes = std::mem::size_of_val(vertices) as u64;
        if bytes > capacity_bytes {
            self.player_caster_count = 0;
            return Err(());
        }
        self.player_caster_count = vertices.len() as u32;
        if vertices.is_empty() {
            return Ok(0);
        }
        let mut allocated = 0;
        if self.player_casters.is_none() {
            self.player_casters = Some(create_caster_buffer(
                device,
                "nearby-shadow-remote-players",
                capacity_bytes,
            ));
            allocated = capacity_bytes;
        }
        if let Some(buffer) = &self.player_casters {
            queue.write_buffer(buffer, 0, bytemuck::cast_slice(vertices));
        }
        Ok(allocated)
    }

    pub fn player_casters(&self) -> Option<(&wgpu::Buffer, u32)> {
        (self.player_caster_count > 0)
            .then(|| {
                self.player_casters
                    .as_ref()
                    .map(|buffer| (buffer, self.player_caster_count))
            })
            .flatten()
    }

    pub fn player_caster_buffer_bytes(&self) -> u64 {
        self.player_casters.as_ref().map_or(0, wgpu::Buffer::size)
    }

    pub fn release_player_casters(&mut self) -> u64 {
        self.player_caster_count = 0;
        self.player_casters.take().map_or(0, |buffer| buffer.size())
    }

    pub fn clear_player_casters(&mut self) {
        self.player_caster_count = 0;
    }

    pub fn write_uniform(&self, queue: &wgpu::Queue, uniform: &ShadowUniform) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniform));
    }

    pub fn gpu_bytes(&self) -> u64 {
        self.gpu_bytes + self.uniform_buffer.size()
    }

    pub fn reconfigure(
        &mut self,
        device: &wgpu::Device,
        settings: ShadowSettings,
    ) -> Result<(), &'static str> {
        if self.settings == settings {
            self.fallback_reason = None;
            return Ok(());
        }
        if settings.enabled && self.format != wgpu::TextureFormat::Depth32Float {
            return Err("standard_profile_requires_depth32float");
        }
        let (texture, depth_view, sampling_bind_group, gpu_bytes) = create_target_checked(
            device,
            &self.group2_layout,
            &self.uniform_buffer,
            &self.sampler,
            settings,
            self.format,
        )?;
        self._texture = texture;
        self.depth_view = depth_view;
        self.sampling_bind_group = sampling_bind_group;
        self.gpu_bytes = gpu_bytes;
        self.settings = settings;
        self.fallback_reason = None;
        Ok(())
    }

    pub fn texture_bytes(&self) -> u64 {
        let resolution = if self.settings.enabled {
            self.settings.resolution.min(DEFAULT_RESOLUTION)
        } else {
            1
        };
        let bytes_per_pixel = if self.format == wgpu::TextureFormat::Depth16Unorm {
            2
        } else {
            4
        };
        u64::from(resolution) * u64::from(resolution) * bytes_per_pixel
    }
}

fn create_caster_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}
