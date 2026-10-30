use super::{sector_scene::ClientSectorScene, GpuResources, Vertex, UI_VERTEX_CAPACITY};
use crate::console::{font_atlas_pixels, UiVertex};
use std::{borrow::Cow, error::Error, sync::Arc};
use wgpu::util::DeviceExt;
use winit::{dpi::PhysicalSize, window::Window};

impl GpuResources {
    pub(super) fn cache_sector_scene(
        &mut self,
        instance: minimium_core::world::WorldInstanceId,
        scene: &ClientSectorScene,
    ) -> bool {
        let version = (scene.generation, scene.content_revision);
        if self
            .world_sector_versions
            .get(&instance)
            .is_some_and(|cached| *cached != version)
        {
            self.release_world_sectors(instance);
        }
        self.world_sector_versions.insert(instance, version);
        let required: std::collections::HashSet<_> = scene
            .render_sectors
            .iter()
            .map(|(key, lod, _)| (*key, *lod))
            .collect();
        for (key, lod, geometry) in &scene.render_sectors {
            if self
                .world_vertex_catalog
                .contains_key(&(instance, *key, *lod))
            {
                continue;
            }
            let vertices = super::geometry::sector_geometry_relative(geometry, self.render_origin);
            let empty_vertices = [0.0_f32; 9];
            let contents = if vertices.is_empty() {
                bytemuck::cast_slice(&empty_vertices)
            } else {
                bytemuck::cast_slice(&vertices)
            };
            let buffer = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("world-sector-geometry"),
                    contents,
                    usage: wgpu::BufferUsages::VERTEX,
                });
            self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_add(buffer.size());
            self.world_vertex_catalog
                .insert((instance, *key, *lod), (buffer, vertices.len() as u32));
        }
        self.world_vertex_catalog
            .retain(|(owner, key, lod), (buffer, _)| {
                if *owner != instance || required.contains(&(*key, *lod)) {
                    true
                } else {
                    self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(buffer.size());
                    false
                }
            });
        if scene.render_sectors.iter().any(|(key, lod, _)| {
            !self
                .world_vertex_catalog
                .contains_key(&(instance, *key, *lod))
        }) {
            return false;
        }
        true
    }

    pub(super) fn activate_sector_scene(
        &mut self,
        instance: minimium_core::world::WorldInstanceId,
        scene: &ClientSectorScene,
    ) -> bool {
        if scene.render_sectors.iter().any(|(key, lod, _)| {
            !self
                .world_vertex_catalog
                .contains_key(&(instance, *key, *lod))
        }) {
            return false;
        }
        self.active_sector_keys = scene
            .render_sectors
            .iter()
            .map(|(key, lod, _)| (*key, *lod))
            .collect();
        self.active_world_instance = instance;
        true
    }

    pub(super) fn release_world_sectors(
        &mut self,
        instance: minimium_core::world::WorldInstanceId,
    ) {
        self.world_vertex_catalog
            .retain(|(owner, _, _), (buffer, _)| {
                if *owner != instance {
                    true
                } else {
                    self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(buffer.size());
                    false
                }
            });
        self.world_sector_versions.remove(&instance);
    }

    pub(super) fn update_remote_vertices(&mut self, vertices: &[Vertex]) {
        self.remote_vertex_count = vertices.len() as u32;
        if self.remote_vertex_count > 0 {
            self.queue
                .write_buffer(&self.player_buffer, 0, bytemuck::cast_slice(vertices));
        }
    }

    pub(super) fn set_render_origin(&mut self, origin: [f32; 3]) -> bool {
        if self.render_origin == origin {
            return false;
        }
        self.render_origin = origin;
        for (_, (buffer, _)) in self.world_vertex_catalog.drain() {
            self.fixed_gpu_bytes = self.fixed_gpu_bytes.saturating_sub(buffer.size());
        }
        self.world_sector_versions.clear();
        self.active_sector_keys.clear();
        true
    }

    pub(super) async fn new(
        window: Arc<Window>,
        vertices: &[Vertex],
    ) -> Result<Self, Box<dyn Error>> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await?;
        let mut required_limits = wgpu::Limits::downlevel_defaults();
        required_limits.max_texture_dimension_2d =
            adapter.limits().max_texture_dimension_2d.min(4096);
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("minimium-device"),
                required_features: wgpu::Features::empty(),
                required_limits,
                experimental_features: Default::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await?;
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("graphics adapter does not support this surface")?;
        surface.configure(&device, &config);
        let output_encoder = (!is_srgb(config.format))
            .then(|| super::output::OutputEncoder::new(&device, size, config.format));
        let scene_color_format = if output_encoder.is_some() {
            wgpu::TextureFormat::Rgba8Unorm
        } else {
            config.format
        };
        let frame_bindings = super::environment_gpu::EnvironmentGpuBindings::create(&device);
        let environment_pipelines = super::sky_gpu::EnvironmentPipelines::new(
            &device,
            &frame_bindings.layout,
            scene_color_format,
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("minimium-basic-3d-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(super::shader_source::world())),
        });
        let pipeline_layout = frame_bindings.pipeline_layout(&device);
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("minimium-solid-3d-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(Vertex::layout())],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: scene_color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
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
        let (ui_pipeline, ui_bind_group, ui_vertex_buffer) =
            create_ui_resources(&device, &queue, scene_color_format);
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sample-world-geometry"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let player_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("network-player-vertices"),
            size: (512 * 36 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth_view = create_depth_view(&device, size);
        let fixed_gpu_bytes = frame_bindings.camera_buffer.size()
            + frame_bindings.environment_buffer.size()
            + vertex_buffer.size()
            + player_buffer.size()
            + ui_vertex_buffer.size()
            + output_encoder
                .as_ref()
                .map_or(0, |output| output.size_bytes())
            + (crate::console::ATLAS_WIDTH * crate::console::ATLAS_HEIGHT) as u64;
        Ok(Self {
            surface,
            device,
            queue,
            config,
            scene_color_format,
            pipeline,
            environment_pipelines,
            ui_pipeline,
            ui_bind_group,
            ui_vertex_buffer,
            ui_vertex_count: 0,
            vertex_buffer,
            vertex_count: vertices.len() as u32,
            player_buffer,
            remote_vertex_count: 0,
            camera_buffer: frame_bindings.camera_buffer,
            environment_buffer: frame_bindings.environment_buffer,
            camera_bind_group: frame_bindings.bind_group,
            depth_view,
            size,
            fixed_gpu_bytes,
            world_vertex_catalog: std::collections::HashMap::new(),
            world_sector_versions: std::collections::HashMap::new(),
            active_sector_keys: Vec::new(),
            active_world_instance: minimium_core::world::WorldInstanceId::new(1)
                .expect("primary world ID is nonzero"),
            render_origin: [0.0; 3],
            overview_buffer: None,
            overview_marker_buffer: None,
            environment_draw_calls: super::sky::EnvironmentDrawCalls::default(),
            overview_active: false,
            overview_globe: false,
            scene_visible: true,
            globe: None,
            output_encoder,
        })
    }

    pub(super) fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.size = size;
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, size);
        if let Some(output) = &mut self.output_encoder {
            let old_size = output.size_bytes();
            output.resize(&self.device, size);
            self.fixed_gpu_bytes = self
                .fixed_gpu_bytes
                .saturating_sub(old_size)
                .saturating_add(output.size_bytes());
        }
    }
}

fn create_ui_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::BindGroup, wgpu::Buffer) {
    let atlas_width = crate::console::ATLAS_WIDTH;
    let atlas_height = crate::console::ATLAS_HEIGHT;
    let atlas = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("minimium-console-font-atlas"),
        size: wgpu::Extent3d {
            width: atlas_width as u32,
            height: atlas_height as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let font = font_atlas_pixels();
    let bytes_per_row = atlas_width.div_ceil(256) * 256;
    let mut upload = vec![0; bytes_per_row * atlas_height];
    for row in 0..atlas_height {
        let row_start = row * bytes_per_row;
        let font_start = row * atlas_width;
        upload[row_start..row_start + atlas_width]
            .copy_from_slice(&font[font_start..font_start + atlas_width]);
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &atlas,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &upload,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row as u32),
            rows_per_image: Some(atlas_height as u32),
        },
        wgpu::Extent3d {
            width: atlas_width as u32,
            height: atlas_height as u32,
            depth_or_array_layers: 1,
        },
    );
    let atlas_view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("minimium-console-font-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("minimium-console-ui-bind-layout"),
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
        ],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("minimium-console-ui-bind-group"),
        layout: &bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&atlas_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("minimium-console-ui-shader"),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(super::shader_source::ui())),
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("minimium-console-ui-pipeline-layout"),
        bind_group_layouts: &[Some(&bind_group_layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("minimium-console-ui-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(UiVertex::layout())],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    });
    let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("minimium-console-ui-vertices"),
        size: (UI_VERTEX_CAPACITY * std::mem::size_of::<UiVertex>()) as u64,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (pipeline, bind_group, vertex_buffer)
}

fn create_depth_view(device: &wgpu::Device, size: PhysicalSize<u32>) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth-buffer"),
            size: wgpu::Extent3d {
                width: size.width.max(1),
                height: size.height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

fn is_srgb(format: wgpu::TextureFormat) -> bool {
    matches!(
        format,
        wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
    )
}
