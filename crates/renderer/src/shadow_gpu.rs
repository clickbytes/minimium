use crate::{
    geometry::Vertex, shadow_target::create_target_checked, shadow_uniform::ShadowUniform,
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
    moon_views: [wgpu::TextureView; 2],
    moon_groups: [wgpu::BindGroup; 2],
    moon_uniforms: [wgpu::Buffer; 2],
    landscape_view: wgpu::TextureView,
    landscape_bind_group: wgpu::BindGroup,
    landscape_uniform: wgpu::Buffer,
    pub sampling_bind_group: wgpu::BindGroup,
    pub empty_bind_group: wgpu::BindGroup,
    pub empty_layout: wgpu::BindGroupLayout,
    pub group2_layout: wgpu::BindGroupLayout,
    pub uniform_buffer: wgpu::Buffer,
    map_casters: Option<wgpu::Buffer>,
    map_caster_count: u32,
    player_casters: Option<wgpu::Buffer>,
    player_caster_count: u32,
    caster_revision: u64,
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
        let group2_layout = crate::shadow_target::sampling_layout(device);
        let empty_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("world-material-reserved-layout"),
            entries: &[],
        });
        let empty_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("world-material-empty-bind-group"),
            layout: &empty_layout,
            entries: &[],
        });
        let create_uniform = |label| {
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytemuck::bytes_of(&ShadowUniform::zeroed()),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(label),
                layout: &uniform_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            (buffer, group)
        };
        let (uniform_buffer, uniform_bind_group) = create_uniform("local-shadow-uniform");
        let (landscape_uniform, landscape_bind_group) = create_uniform("landscape-shadow-uniform");
        let [(moon_near, near_group), (moon_far, far_group)] =
            ["secondary-moon-contact", "secondary-moon-landscape"].map(create_uniform);
        let moon_uniforms = [moon_near, moon_far];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("shadow-comparison-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
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
            &landscape_uniform,
            &moon_uniforms,
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
                    &landscape_uniform,
                    &moon_uniforms,
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
        Ok(Self {
            settings,
            fallback_reason,
            format,
            depth_pipeline,
            depth_view: target.local_view,
            uniform_bind_group,
            moon_views: target.moon_views,
            moon_groups: [near_group, far_group],
            moon_uniforms,
            landscape_view: target.landscape_view,
            landscape_uniform,
            landscape_bind_group,
            sampling_bind_group: target.sampling,
            empty_bind_group,
            empty_layout,
            group2_layout,
            uniform_buffer,
            map_casters: None,
            map_caster_count: 0,
            player_casters: None,
            player_caster_count: 0,
            caster_revision: 0,
            _texture: target.texture,
            sampler,
            gpu_bytes: target.bytes,
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
        self.encode_map(
            encoder,
            &self.depth_view,
            &self.uniform_bind_group,
            casters,
            "local-shadow-depth-pass",
        )
    }

    pub(crate) fn encode_landscape_depth<'a>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        casters: impl IntoIterator<Item = (&'a wgpu::Buffer, u32)>,
    ) -> u32 {
        self.encode_map(
            encoder,
            &self.landscape_view,
            &self.landscape_bind_group,
            casters,
            "landscape-shadow-depth-pass",
        )
    }

    pub(crate) fn encode_secondary_moon<'a>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        cascade: usize,
        casters: impl IntoIterator<Item = (&'a wgpu::Buffer, u32)>,
    ) -> u32 {
        self.encode_map(
            encoder,
            &self.moon_views[cascade],
            &self.moon_groups[cascade],
            casters,
            "secondary-moon-shadow-depth",
        )
    }

    fn encode_map<'a>(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        uniform: &wgpu::BindGroup,
        casters: impl IntoIterator<Item = (&'a wgpu::Buffer, u32)>,
        label: &str,
    ) -> u32 {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view,
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
        pass.set_bind_group(0, uniform, &[]);
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
        if self.map_caster_count != 0 || !vertices.is_empty() {
            self.caster_revision = self.caster_revision.wrapping_add(1);
        }
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
        self.clear_map_casters();
        self.map_casters.take().map_or(0, |buffer| buffer.size())
    }

    pub fn clear_map_casters(&mut self) {
        if self.map_caster_count != 0 {
            self.caster_revision = self.caster_revision.wrapping_add(1);
        }
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
        if self.player_caster_count != 0 || !vertices.is_empty() {
            self.caster_revision = self.caster_revision.wrapping_add(1);
        }
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
        self.clear_player_casters();
        self.player_casters.take().map_or(0, |buffer| buffer.size())
    }

    pub fn clear_player_casters(&mut self) {
        if self.player_caster_count != 0 {
            self.caster_revision = self.caster_revision.wrapping_add(1);
        }
        self.player_caster_count = 0;
    }

    /// Changes when auxiliary map or moving player caster data is republished.
    pub fn caster_revision(&self) -> u64 {
        self.caster_revision
    }

    pub fn write_uniform(&self, queue: &wgpu::Queue, uniform: &ShadowUniform) {
        self.write_uniforms(queue, uniform, None);
    }

    pub fn write_uniforms(
        &self,
        queue: &wgpu::Queue,
        uniform: &ShadowUniform,
        landscape: Option<&ShadowUniform>,
    ) {
        self.write_cascades(queue, uniform, landscape, [None; 2]);
    }

    pub fn write_cascades(
        &self,
        queue: &wgpu::Queue,
        uniform: &ShadowUniform,
        landscape: Option<&ShadowUniform>,
        moon: [Option<&ShadowUniform>; 2],
    ) {
        for (buffer, value) in self.moon_uniforms.iter().zip(moon) {
            queue.write_buffer(
                buffer,
                0,
                bytemuck::bytes_of(value.unwrap_or(&ShadowUniform::default())),
            );
        }
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(uniform));
        queue.write_buffer(
            &self.landscape_uniform,
            0,
            bytemuck::bytes_of(landscape.unwrap_or(&ShadowUniform::default())),
        );
    }

    pub fn uniform_bytes(&self) -> u64 {
        self.uniform_buffer.size()
            + self.landscape_uniform.size()
            + self
                .moon_uniforms
                .iter()
                .map(wgpu::Buffer::size)
                .sum::<u64>()
    }

    pub fn gpu_bytes(&self) -> u64 {
        self.texture_bytes() + self.uniform_bytes()
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
        let target = create_target_checked(
            device,
            &self.group2_layout,
            &self.uniform_buffer,
            &self.landscape_uniform,
            &self.moon_uniforms,
            &self.sampler,
            settings,
            self.format,
        )?;
        self._texture = target.texture;
        self.depth_view = target.local_view;
        self.landscape_view = target.landscape_view;
        self.moon_views = target.moon_views;
        self.sampling_bind_group = target.sampling;
        self.gpu_bytes = target.bytes;
        self.settings = settings;
        self.fallback_reason = None;
        Ok(())
    }

    pub fn texture_bytes(&self) -> u64 {
        self.gpu_bytes
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
