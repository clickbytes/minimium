use super::{
    environment::{EnvironmentFrame, EnvironmentFrameStatus, EnvironmentViewKind},
    geometry::Vertex,
    shadows::{self, ShadowInvalidReason, ShadowSettings, ShadowUniform},
    GpuResources,
};
use bytemuck::Zeroable;
use wgpu::util::DeviceExt;

const DEFAULT_RESOLUTION: u32 = 1024;

pub(super) struct ShadowGpu {
    pub(super) settings: ShadowSettings,
    pub(super) depth_pipeline: wgpu::RenderPipeline,
    pub(super) depth_view: wgpu::TextureView,
    pub(super) uniform_bind_group: wgpu::BindGroup,
    pub(super) sampling_bind_group: wgpu::BindGroup,
    pub(super) empty_bind_group: wgpu::BindGroup,
    pub(super) empty_layout: wgpu::BindGroupLayout,
    pub(super) group2_layout: wgpu::BindGroupLayout,
    pub(super) uniform_buffer: wgpu::Buffer,
    _texture: wgpu::Texture,
    sampler: wgpu::Sampler,
    gpu_bytes: u64,
}

impl ShadowGpu {
    pub(super) fn new(device: &wgpu::Device, settings: ShadowSettings) -> Self {
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
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
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
        let (texture, depth_view, sampling_bind_group, gpu_bytes) =
            create_target(device, &group2_layout, &uniform_buffer, &sampler, settings);
        Self {
            settings,
            depth_pipeline,
            depth_view,
            uniform_bind_group,
            sampling_bind_group,
            empty_bind_group,
            empty_layout,
            group2_layout,
            uniform_buffer,
            _texture: texture,
            sampler,
            gpu_bytes,
        }
    }

    pub(super) fn write_uniform(&self, queue: &wgpu::Queue, uniform: &ShadowUniform) {
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniform));
    }

    pub(super) fn gpu_bytes(&self) -> u64 {
        self.gpu_bytes + self.uniform_buffer.size()
    }

    pub(super) fn reconfigure(&mut self, device: &wgpu::Device, settings: ShadowSettings) {
        if self.settings == settings {
            return;
        }
        let (texture, depth_view, sampling_bind_group, gpu_bytes) = create_target(
            device,
            &self.group2_layout,
            &self.uniform_buffer,
            &self.sampler,
            settings,
        );
        self._texture = texture;
        self.depth_view = depth_view;
        self.sampling_bind_group = sampling_bind_group;
        self.gpu_bytes = gpu_bytes;
        self.settings = settings;
    }
}

impl GpuResources {
    #[allow(dead_code)]
    pub(super) fn set_shadow_settings(&mut self, settings: ShadowSettings) {
        let previous_bytes = self.shadow_gpu.gpu_bytes();
        self.shadow_gpu.reconfigure(&self.device, settings);
        let current_bytes = self.shadow_gpu.gpu_bytes();
        self.fixed_gpu_bytes = self
            .fixed_gpu_bytes
            .saturating_sub(previous_bytes)
            .saturating_add(current_bytes);
        if !settings.enabled {
            self.release_shadow_map_casters();
            if let Some(buffer) = self.shadow_player_buffer.take() {
                self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(buffer.size());
            }
            self.shadow_player_vertex_count = 0;
            self.shadow_remote_complete = false;
            self.shadow_remote_dirty = true;
            self.shadow_state.invalidate(ShadowInvalidReason::Disabled);
        }
    }

    pub(super) fn update_remote_shadow_vertices(&mut self, vertices: &[Vertex]) {
        if !self.shadow_gpu.settings.enabled {
            self.shadow_player_vertex_count = 0;
            self.shadow_remote_complete = false;
            self.shadow_remote_dirty = true;
            return;
        }
        self.shadow_remote_complete = true;
        self.shadow_remote_dirty = false;
        self.shadow_player_vertex_count = vertices.len() as u32;
        if vertices.is_empty() {
            return;
        }
        let size = (super::MAX_VISIBLE_PLAYERS * 36 * std::mem::size_of::<Vertex>()) as u64;
        let bytes = std::mem::size_of_val(vertices) as u64;
        if bytes > size {
            self.shadow_remote_complete = false;
            self.shadow_remote_dirty = true;
            self.shadow_state
                .invalidate(ShadowInvalidReason::BudgetExceeded);
            self.shadow_player_vertex_count = 0;
            return;
        }
        if self.shadow_player_buffer.is_none() {
            let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("nearby-shadow-remote-players"),
                size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(buffer.size());
            self.shadow_player_buffer = Some(buffer);
        }
        if let Some(buffer) = &self.shadow_player_buffer {
            self.queue
                .write_buffer(buffer, 0, bytemuck::cast_slice(vertices));
        }
    }

    pub(super) fn prepare_shadow_frame(
        &mut self,
        environment: &EnvironmentFrame,
        map: Option<&minimium_core::world::WorldMap>,
        player_position_m: [f64; 3],
    ) {
        self.shadow_state.caster_keys.clear();
        self.shadow_state.valid = false;
        self.shadow_state.source = None;
        self.shadow_state.invalid_reason = None;
        if !self.shadow_gpu.settings.enabled {
            self.shadow_state.invalidate(ShadowInvalidReason::Disabled);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        if environment.status != EnvironmentFrameStatus::Available
            || environment.render_view.kind != EnvironmentViewKind::Surface
            || !self.scene_visible
            || self.overview_active
        {
            self.shadow_state
                .invalidate(ShadowInvalidReason::InvalidFrame);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        let Some(context) = environment.context else {
            self.shadow_state
                .invalidate(ShadowInvalidReason::StaleContext);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        if context.instance() != self.active_world_instance
            || self
                .world_sector_versions
                .get(&self.active_world_instance)
                .is_none_or(|(_, revision)| *revision != context.content_revision())
        {
            self.shadow_state
                .invalidate(ShadowInvalidReason::StaleContext);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        let map = match map {
            Some(map) => map,
            None => {
                self.shadow_state
                    .invalidate(ShadowInvalidReason::StaleContext);
                self.shadow_gpu
                    .write_uniform(&self.queue, &ShadowUniform::zeroed());
                return;
            }
        };
        let source = shadows::select_source(environment, self.shadow_state.previous_source);
        let Some(source) = source else {
            self.shadow_state.previous_source = None;
            self.shadow_state.invalidate(ShadowInvalidReason::NoSource);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        self.update_shadow_map_vertices(map, player_position_m);
        if self.shadow_state.invalid_reason == Some(ShadowInvalidReason::BudgetExceeded) {
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        if !self.shadow_remote_complete {
            self.shadow_state
                .invalidate(ShadowInvalidReason::IncompleteCasters);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        if self.active_sector_keys.is_empty() {
            self.shadow_state
                .invalidate(ShadowInvalidReason::IncompleteCasters);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        let radius = f64::from(self.shadow_gpu.settings.caster_radius_m);
        for (key, lod) in &self.active_sector_keys {
            let catalog_key = (self.active_world_instance, *key, *lod);
            if !self.world_vertex_catalog.contains_key(&catalog_key) {
                self.shadow_state
                    .invalidate(ShadowInvalidReason::IncompleteCasters);
                self.shadow_gpu
                    .write_uniform(&self.queue, &ShadowUniform::zeroed());
                return;
            }
            let Some(bounds) = self.world_geometry_bounds.get(&catalog_key) else {
                self.shadow_state
                    .invalidate(ShadowInvalidReason::IncompleteCasters);
                self.shadow_gpu
                    .write_uniform(&self.queue, &ShadowUniform::zeroed());
                return;
            };
            if bounds.intersects_sphere(player_position_m, radius) {
                self.shadow_state.caster_keys.push((*key, *lod));
            }
        }
        if self.shadow_state.caster_keys.is_empty() {
            self.shadow_state
                .invalidate(ShadowInvalidReason::IncompleteCasters);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        }
        let Some(direction) = source.direction(environment) else {
            self.shadow_state
                .invalidate(ShadowInvalidReason::InvalidFrame);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        let center_relative = std::array::from_fn(|axis| {
            player_position_m[axis] - f64::from(self.render_origin[axis])
        });
        let Some(uniform) =
            shadows::shadow_uniform(direction, center_relative, self.shadow_gpu.settings, source)
        else {
            self.shadow_state
                .invalidate(ShadowInvalidReason::InvalidFrame);
            self.shadow_gpu
                .write_uniform(&self.queue, &ShadowUniform::zeroed());
            return;
        };
        self.shadow_state.previous_source = Some(source);
        self.shadow_state.source = Some(source);
        self.shadow_state.valid = true;
        self.shadow_state.invalid_reason = None;
        self.shadow_gpu.write_uniform(&self.queue, &uniform);
    }
}

fn create_target(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    uniform: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    settings: ShadowSettings,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup, u64) {
    let resolution = if settings.enabled {
        settings.resolution.clamp(1, DEFAULT_RESOLUTION)
    } else {
        1
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("local-shadow-depth-map"),
        size: wgpu::Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let depth_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let sampling_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("world-shadow-sampling-bind-group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&depth_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniform.as_entire_binding(),
            },
        ],
    });
    let bytes = u64::from(resolution) * u64::from(resolution) * 4;
    (texture, depth_view, sampling_bind_group, bytes)
}

fn uniform_layout_entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(128),
        },
        count: None,
    }
}
