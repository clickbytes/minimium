use super::*;
use std::borrow::Cow;
use wgpu::util::DeviceExt;

impl Renderer {
    pub(super) async fn new(
        window: Arc<Window>,
        map: WorldMap,
        profile_id: PlayerProfileId,
        data_dir: PathBuf,
        map_path: Option<PathBuf>,
        threads: crate::options::ThreadLane,
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
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("minimium-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                experimental_features: Default::default(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                trace: wgpu::Trace::Off,
            })
            .await?;
        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .ok_or("graphics adapter does not support this surface")?;
        surface.configure(&device, &config);

        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("camera-uniform"),
            size: std::mem::size_of::<[[f32; 4]; 4]>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera-bind-group"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("minimium-basic-3d-shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("world.wgsl"))),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("minimium-pipeline-layout"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
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
                    format: config.format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
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
            create_ui_resources(&device, &queue, config.format);
        let vertices = map_geometry(&map);
        let map_fingerprint = map.fingerprint();
        let collision_index = map.collision_index();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sample-world-geometry"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let player_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("network-player-vertices"),
            size: (512 * 36 * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let depth_view = create_depth_view(&device, size);
        let fixed_gpu_bytes = camera_buffer.size()
            + vertex_buffer.size()
            + player_buffer.size()
            + ui_vertex_buffer.size()
            + (crate::console::ATLAS_WIDTH * crate::console::ATLAS_HEIGHT) as u64;
        let mut frame_stats = crate::stats::FrameStats::default();
        frame_stats.set_gpu_bytes(
            fixed_gpu_bytes + u64::from(size.width.max(1)) * u64::from(size.height.max(1)) * 4,
        );
        let mut console = Console::new();
        console.push_line("F1 opens the command console. Type help for commands.");
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            ui_pipeline,
            ui_bind_group,
            ui_vertex_buffer,
            ui_vertex_count: 0,
            console,
            vertex_buffer,
            vertex_count: vertices.len() as u32,
            player_buffer,
            remote_vertex_count: 0,
            remote_vertices: Vec::with_capacity(16 * 36),
            remote_players: [None; MAX_VISIBLE_PLAYERS],
            last_remote_snapshot_time_ms: None,
            last_remote_snapshot_received_at: None,
            remote_geometry_dirty: false,
            camera_buffer,
            camera_bind_group,
            depth_view,
            size,
            character: CharacterState::at_spawn(map.spawn()),
            health: PLAYER_MAX_HEALTH,
            collision_index,
            collision_query: CollisionQuery::default(),
            yaw: -std::f32::consts::FRAC_PI_2,
            pitch: 0.0,
            keys: HashSet::new(),
            last_cursor: None,
            cursor_locked: false,
            last_frame: Instant::now(),
            fps_time: Instant::now(),
            fps_frames: 0,
            frame_stats,
            fixed_gpu_bytes,
            previous_bytes_sent: 0,
            previous_bytes_received: 0,
            last_redraw: Instant::now(),
            remote_player_count: 0,
            connection: ConnectionController::new(profile_id, map_fingerprint),
            session_events: Vec::with_capacity(16),
            last_shot_sent_at: None,
            hit_confirmation_until: None,
            data_dir,
            map_path,
            threads,
            physics_accumulator: 0.0,
            jump_pending: false,
            input_keepalive: 0.0,
            last_sent_direction: [0.0; 2],
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
        self.frame_stats.set_gpu_bytes(
            self.fixed_gpu_bytes + u64::from(size.width) * u64::from(size.height) * 4,
        );
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
        mag_filter: wgpu::FilterMode::Nearest,
        min_filter: wgpu::FilterMode::Nearest,
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
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
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
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(include_str!("ui.wgsl"))),
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
