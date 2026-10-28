use crate::{
    console::{font_atlas_pixels, parse_command, ClientCommand, Console, UiVertex},
    server_error_text, ConnectionState, WELCOME_TIMEOUT,
};
use minimium_core::{
    identity::PlayerProfileId,
    world::{
        step_character, CharacterInput, CharacterState, CollisionIndex, CollisionQuery, WorldMap,
        PLAYER_MAX_HEALTH,
    },
};
use minimium_protocol::{
    decode_server, encode_client, ClientMessage, ServerMessage, MAX_VISIBLE_PLAYERS,
};
use std::{
    borrow::Cow,
    collections::HashSet,
    error::Error,
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
    time::{Duration, Instant},
};
use wgpu::util::DeviceExt;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowAttributes, WindowId},
};

const MAX_NETWORK_BYTES_PER_FRAME: usize = 16 * 1024;
const MAX_NETWORK_FRAME_BYTES: usize = 64 * 1024;
const UI_VERTEX_CAPACITY: usize = 8192;
const REMOTE_INTERPOLATION_DELAY: Duration = Duration::from_millis(100);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(20);
const SHOT_LOCAL_COOLDOWN: Duration = Duration::from_millis(300);
const SAMPLE_MAP: &str = include_str!("../../../games/sample/maps/demo.map");

const SHADER: &str = r#"
struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_projection * vec4<f32>(input.position, 1.0);
    out.color = input.color;
    return out;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}
"#;

const UI_SHADER: &str = r#"
struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec3<f32>,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
};
@group(0) @binding(0) var font_atlas: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(input.position, 0.0, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    return out;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(font_atlas, font_sampler, input.uv).r;
    return vec4<f32>(input.color, alpha);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3];
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

#[derive(Clone, Copy)]
struct RemotePlayerTrack {
    id: u64,
    previous_position: [f32; 3],
    current_position: [f32; 3],
    displayed_position: [f32; 3],
    previous_time_ms: u64,
    current_time_ms: u64,
    displayed_time_ms: u64,
}

impl UiVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x3];
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    ui_pipeline: wgpu::RenderPipeline,
    ui_bind_group: wgpu::BindGroup,
    ui_vertex_buffer: wgpu::Buffer,
    ui_vertex_count: u32,
    console: Console,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    player_buffer: wgpu::Buffer,
    remote_vertex_count: u32,
    remote_vertices: Vec<Vertex>,
    remote_players: [Option<RemotePlayerTrack>; MAX_VISIBLE_PLAYERS],
    last_remote_snapshot_time_ms: Option<u64>,
    last_remote_snapshot_received_at: Option<Instant>,
    remote_geometry_dirty: bool,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    size: PhysicalSize<u32>,
    character: CharacterState,
    health: u16,
    collision_index: CollisionIndex,
    collision_query: CollisionQuery,
    yaw: f32,
    pitch: f32,
    keys: HashSet<KeyCode>,
    last_cursor: Option<(f64, f64)>,
    cursor_locked: bool,
    last_frame: Instant,
    fps_time: Instant,
    fps_frames: u32,
    frame_stats: crate::stats::FrameStats,
    last_redraw: Instant,
    fixed_gpu_bytes: u64,
    bytes_sent: u64,
    bytes_received: u64,
    remote_player_count: usize,
    map_fingerprint: u64,
    profile_id: PlayerProfileId,
    connection: Option<TcpStream>,
    connection_state: ConnectionState,
    last_heartbeat_sent_at: Instant,
    pending_ping_sent_at: Option<Instant>,
    round_trip_ms: Option<u32>,
    last_shot_sent_at: Option<Instant>,
    hit_confirmation_until: Option<Instant>,
    receive_buffer: Vec<u8>,
    send_buffer: Vec<u8>,
    send_offset: usize,
    physics_accumulator: f32,
    jump_pending: bool,
    input_keepalive: f32,
    last_sent_direction: [f32; 2],
}

impl Renderer {
    async fn new(
        window: Arc<Window>,
        connection: Option<(TcpStream, Instant)>,
        map: WorldMap,
        profile_id: PlayerProfileId,
    ) -> Result<Self, Box<dyn Error>> {
        let (connection, connection_state) = match connection {
            Some((stream, since)) => (Some(stream), ConnectionState::AwaitingWelcome { since }),
            None => (None, ConnectionState::Disconnected),
        };
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
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(SHADER)),
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
            + 128 * 64;
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
            character: CharacterState::at_spawn(map.spawn),
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
            last_redraw: Instant::now(),
            bytes_sent: 0,
            bytes_received: 0,
            remote_player_count: 0,
            map_fingerprint,
            profile_id,
            connection,
            connection_state,
            last_heartbeat_sent_at: Instant::now(),
            pending_ping_sent_at: None,
            round_trip_ms: None,
            last_shot_sent_at: None,
            hit_confirmation_until: None,
            receive_buffer: Vec::with_capacity(8192),
            send_buffer: Vec::with_capacity(256),
            send_offset: 0,
            physics_accumulator: 0.0,
            jump_pending: false,
            input_keepalive: 0.0,
            last_sent_direction: [0.0; 2],
        })
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
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

    fn update(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.05);
        self.last_frame = now;
        let forward = glam::Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        let right = glam::Vec3::new(-forward.z, 0.0, forward.x);
        let mut movement = glam::Vec3::ZERO;
        if !self.console.open {
            if self.keys.contains(&KeyCode::KeyW) {
                movement += forward;
            }
            if self.keys.contains(&KeyCode::KeyS) {
                movement -= forward;
            }
            if self.keys.contains(&KeyCode::KeyD) {
                movement += right;
            }
            if self.keys.contains(&KeyCode::KeyA) {
                movement -= right;
            }
        }
        let direction = if movement.length_squared() > 1.0 {
            movement.normalize()
        } else {
            movement
        };
        self.physics_accumulator += dt;
        while self.physics_accumulator >= 0.05 {
            let input = CharacterInput {
                direction: [direction.x, direction.z],
                jump: self.jump_pending,
            };
            self.jump_pending = false;
            self.collision_query
                .fill(&self.collision_index, self.character, 0.05);
            step_character(
                &mut self.character,
                input,
                self.collision_query.candidates(),
                self.collision_query.surface_candidates(),
                0.05,
            );
            self.input_keepalive += 0.05;
            let moving = input.direction != [0.0, 0.0];
            let input_changed = input.direction != self.last_sent_direction;
            if self.connection_state == ConnectionState::Connected
                && self.connection.is_some()
                && (input.jump || input_changed || (moving && self.input_keepalive >= 0.1))
            {
                if let Ok(frame) = encode_client(&ClientMessage::Move {
                    direction: input.direction,
                    jump: input.jump,
                }) {
                    self.queue_network_command(&frame);
                }
                self.last_sent_direction = input.direction;
                self.input_keepalive = 0.0;
            }
            self.physics_accumulator -= 0.05;
        }
        self.queue_heartbeat(now);
        self.flush_network_commands();
        self.receive_network_state();
        self.update_remote_player_rendering(Instant::now());

        let direction = glam::Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize();
        let camera_position = glam::Vec3::new(
            self.character.position[0],
            self.character.position[1] + 1.7,
            self.character.position[2],
        );
        let view = glam::Mat4::look_to_rh(camera_position, direction, glam::Vec3::Y);
        let projection = glam::Mat4::perspective_rh(
            70f32.to_radians(),
            self.size.width as f32 / self.size.height as f32,
            0.05,
            200.0,
        );
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&[(projection * view).to_cols_array_2d()]),
        );

        let vertex_count = {
            let vertices = self.console.build_vertices(
                self.size.width,
                self.size.height,
                self.health,
                self.cursor_locked && !self.console.open,
                self.hit_confirmation_until
                    .is_some_and(|until| Instant::now() < until),
            );
            let count = vertices.len() as u32;
            if count > 0 {
                self.queue
                    .write_buffer(&self.ui_vertex_buffer, 0, bytemuck::cast_slice(vertices));
            }
            count
        };
        self.ui_vertex_count = vertex_count;
    }

    fn render(&mut self) -> Result<u32, wgpu::CurrentSurfaceTexture> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.config);
                frame
            }
            error @ (wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost) => {
                return Err(error);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(0);
            }
            error @ wgpu::CurrentSurfaceTexture::Validation => return Err(error),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("frame-encoder"),
            });
        let mut draw_calls = 0;
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.055,
                            g: 0.075,
                            b: 0.105,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
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
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.draw(0..self.vertex_count, 0..1);
            draw_calls += 1;
            if self.remote_vertex_count > 0 {
                pass.set_vertex_buffer(0, self.player_buffer.slice(..));
                pass.draw(0..self.remote_vertex_count, 0..1);
                draw_calls += 1;
            }
        }
        if self.ui_vertex_count > 0 {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("minimium-console-ui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.ui_pipeline);
            pass.set_bind_group(0, &self.ui_bind_group, &[]);
            pass.set_vertex_buffer(0, self.ui_vertex_buffer.slice(..));
            pass.draw(0..self.ui_vertex_count, 0..1);
            draw_calls += 1;
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        Ok(draw_calls)
    }

    fn clear_connection(&mut self) {
        self.connection = None;
        self.connection_state = ConnectionState::Disconnected;
        self.pending_ping_sent_at = None;
        self.round_trip_ms = None;
        self.last_shot_sent_at = None;
        self.receive_buffer.clear();
        self.send_buffer.clear();
        self.send_offset = 0;
        self.remote_vertex_count = 0;
        self.remote_player_count = 0;
        self.remote_vertices.clear();
        self.remote_players = [None; MAX_VISIBLE_PLAYERS];
        self.last_remote_snapshot_time_ms = None;
        self.last_remote_snapshot_received_at = None;
        self.remote_geometry_dirty = false;
        self.health = PLAYER_MAX_HEALTH;
        self.hit_confirmation_until = None;
    }

    fn fail_connection(&mut self, message: impl Into<String>) {
        let message = message.into();
        eprintln!("{message}");
        self.console.push_error(message);
        self.clear_connection();
    }

    fn receive_network_state(&mut self) {
        if let ConnectionState::AwaitingWelcome { since } = self.connection_state {
            if Instant::now().saturating_duration_since(since) >= WELCOME_TIMEOUT {
                self.fail_connection("server welcome timed out; you can reconnect");
                return;
            }
        }
        if self.connection.is_none() {
            self.connection_state = ConnectionState::Disconnected;
            return;
        }
        let mut chunk = [0u8; 4096];
        let mut total_read = 0;
        loop {
            let available = (MAX_NETWORK_BYTES_PER_FRAME - total_read).min(chunk.len());
            if available == 0 {
                break;
            }
            let read_result = match self.connection.as_mut() {
                Some(stream) => stream.read(&mut chunk[..available]),
                None => return,
            };
            match read_result {
                Ok(0) => {
                    self.fail_connection("server disconnected; you can reconnect");
                    return;
                }
                Ok(count) => {
                    total_read += count;
                    self.bytes_received = self.bytes_received.saturating_add(count as u64);
                    self.receive_buffer.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    self.fail_connection(format!("connection lost ({error}); you can reconnect"));
                    return;
                }
            }
        }
        if self.receive_buffer.len() > MAX_NETWORK_FRAME_BYTES {
            self.fail_connection("server frame exceeds protocol limits; you can reconnect");
            return;
        }
        loop {
            let decoded = decode_server(&self.receive_buffer);
            match decoded {
                Ok(Some((message, frame_bytes))) => {
                    self.receive_buffer.drain(..frame_bytes);
                    if !matches!(
                        &message,
                        ServerMessage::Welcome { .. } | ServerMessage::Error(_)
                    ) && self.connection_state != ConnectionState::Connected
                    {
                        self.fail_connection("server sent data before welcome; connection closed");
                        return;
                    }
                    match message {
                        ServerMessage::Welcome {
                            id,
                            map_fingerprint,
                            health,
                        } => {
                            if !matches!(
                                self.connection_state,
                                ConnectionState::AwaitingWelcome { .. }
                            ) {
                                self.fail_connection(
                                    "unexpected server welcome; connection closed",
                                );
                                return;
                            }
                            if map_fingerprint != self.map_fingerprint {
                                let error = format!(
                                    "map mismatch: client={:016x} server={map_fingerprint:016x}",
                                    self.map_fingerprint
                                );
                                eprintln!("{error}");
                                self.fail_connection(error);
                                return;
                            }
                            self.connection_state = ConnectionState::Connected;
                            self.last_heartbeat_sent_at = Instant::now();
                            self.health = health;
                            if self.console.open {
                                self.console.push_line(format!(
                                    "connected as player {id} with {health} HP"
                                ));
                            }
                        }
                        ServerMessage::Pong => {
                            if let Some(sent_at) = self.pending_ping_sent_at.take() {
                                let rtt_ms = u32::try_from(
                                    Instant::now().duration_since(sent_at).as_millis(),
                                )
                                .unwrap_or(u32::MAX);
                                self.round_trip_ms = Some(rtt_ms);
                                if self.console.open {
                                    self.console.push_line(format!("PONG {rtt_ms} ms"));
                                }
                            } else if self.console.open {
                                self.console.push_line("PONG");
                            }
                        }
                        ServerMessage::Accepted if self.console.open => {
                            self.console.push_line("server accepted command");
                        }
                        ServerMessage::Error(code) => {
                            let message = server_error_text(code);
                            if self.connection_state != ConnectionState::Connected {
                                self.fail_connection(format!(
                                    "server rejected connection: {message}"
                                ));
                                return;
                            }
                            self.console.push_error(format!("server error: {message}"));
                        }
                        ServerMessage::ShotResult {
                            target_id,
                            remaining_health,
                            killed,
                        } => {
                            if target_id.is_some() {
                                self.hit_confirmation_until =
                                    Some(Instant::now() + Duration::from_millis(180));
                            }
                            if self.console.open {
                                match (target_id, remaining_health, killed) {
                                    (Some(target_id), Some(health), true) => {
                                        self.console.push_line(format!(
                                            "hit player {target_id}; target respawned ({health} HP)"
                                        ))
                                    }
                                    (Some(target_id), Some(health), false) => self
                                        .console
                                        .push_line(format!("hit player {target_id} ({health} HP)")),
                                    _ => self.console.push_line("shot missed"),
                                }
                            }
                        }
                        ServerMessage::HealthUpdate(health) => {
                            self.health = health;
                            if self.console.open {
                                self.console.push_line(format!("health: {health} HP"));
                            }
                        }
                        ServerMessage::Chat {
                            sender_id,
                            sender_name,
                            text,
                        } => self
                            .console
                            .push_chat_line(&format!("[{sender_name}#{sender_id}] {text}")),
                        ServerMessage::Bye => {
                            if self.console.open {
                                self.console.push_line("server closed the session");
                            }
                            self.clear_connection();
                            return;
                        }
                        ServerMessage::Snapshot {
                            server_time_ms,
                            full_snapshot,
                            self_position,
                            self_velocity,
                            self_grounded,
                            players,
                            removed_players,
                        } => {
                            let server_time_ms = self
                                .last_remote_snapshot_time_ms
                                .map_or(server_time_ms, |previous| previous.max(server_time_ms));
                            self.last_remote_snapshot_time_ms = Some(server_time_ms);
                            self.last_remote_snapshot_received_at = Some(Instant::now());
                            if full_snapshot {
                                self.remote_players = [None; MAX_VISIBLE_PLAYERS];
                            }
                            for id in removed_players.iter() {
                                if let Some(slot) = self
                                    .remote_players
                                    .iter_mut()
                                    .find(|slot| slot.is_some_and(|track| track.id == *id))
                                {
                                    *slot = None;
                                }
                            }
                            for player in players.iter() {
                                let existing_slot = self.remote_players.iter().position(|slot| {
                                    slot.is_some_and(|track| track.id == player.id)
                                });
                                let slot = existing_slot.or_else(|| {
                                    self.remote_players.iter().position(Option::is_none)
                                });
                                let Some(slot) = slot else {
                                    eprintln!("server exceeded the visible-player snapshot limit");
                                    self.fail_connection(
                                        "server exceeded the visible-player snapshot limit",
                                    );
                                    return;
                                };
                                let existing = self.remote_players[slot];
                                let previous_position = existing
                                    .map(|track| track.current_position)
                                    .unwrap_or(player.position);
                                let previous_time_ms = existing
                                    .map(|track| track.current_time_ms.min(server_time_ms))
                                    .unwrap_or(server_time_ms);
                                self.remote_players[slot] = Some(RemotePlayerTrack {
                                    id: player.id,
                                    previous_position,
                                    current_position: player.position,
                                    displayed_position: previous_position,
                                    previous_time_ms,
                                    current_time_ms: server_time_ms,
                                    displayed_time_ms: previous_time_ms,
                                });
                            }
                            self.remote_player_count = self.remote_players.iter().flatten().count();
                            self.remote_geometry_dirty = true;
                            let predicted = glam::Vec3::from_array(self.character.position);
                            let authoritative = glam::Vec3::from_array(self_position);
                            self.character.position =
                                predicted.lerp(authoritative, 0.25).to_array();
                            self.character.velocity = self_velocity;
                            self.character.grounded = self_grounded;
                        }
                        _ => {}
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    self.fail_connection(format!("invalid game protocol data: {error}"));
                    return;
                }
            }
        }
    }

    fn update_remote_player_rendering(&mut self, now: Instant) {
        if self.connection.is_none() {
            self.connection_state = ConnectionState::Disconnected;
            self.remote_players = [None; MAX_VISIBLE_PLAYERS];
            self.last_remote_snapshot_time_ms = None;
            self.last_remote_snapshot_received_at = None;
            self.pending_ping_sent_at = None;
            self.round_trip_ms = None;
            self.last_shot_sent_at = None;
            self.remote_player_count = 0;
            self.remote_vertex_count = 0;
            self.remote_vertices.clear();
            self.remote_geometry_dirty = false;
            return;
        }

        let estimated_server_time_ms = match (
            self.last_remote_snapshot_time_ms,
            self.last_remote_snapshot_received_at,
        ) {
            (Some(server_time_ms), Some(received_at)) => {
                let elapsed_ms =
                    u64::try_from(now.saturating_duration_since(received_at).as_millis())
                        .unwrap_or(u64::MAX);
                server_time_ms.saturating_add(elapsed_ms)
            }
            _ => 0,
        };
        let render_time_ms =
            estimated_server_time_ms.saturating_sub(REMOTE_INTERPOLATION_DELAY.as_millis() as u64);

        let mut moved = false;
        for track in self.remote_players.iter_mut().flatten() {
            let interval_ms = track.current_time_ms.saturating_sub(track.previous_time_ms);
            let interpolation = if interval_ms == 0 {
                1.0
            } else {
                (render_time_ms.saturating_sub(track.previous_time_ms) as f64 / interval_ms as f64)
                    .clamp(0.0, 1.0) as f32
            };
            let position = glam::Vec3::from_array(track.previous_position)
                .lerp(
                    glam::Vec3::from_array(track.current_position),
                    interpolation,
                )
                .to_array();
            if position != track.displayed_position {
                track.displayed_position = position;
                moved = true;
            }
            track.displayed_time_ms = render_time_ms.clamp(
                track.previous_time_ms,
                track.current_time_ms.max(track.previous_time_ms),
            );
        }
        if !moved && !self.remote_geometry_dirty {
            return;
        }

        self.remote_vertices.clear();
        for track in self.remote_players.iter().flatten() {
            let position = track.displayed_position;
            cube(
                &mut self.remote_vertices,
                [position[0], position[1] + 0.85, position[2]],
                [0.6, 1.7, 0.6],
                [0.82, 0.16, 0.12],
            );
        }
        self.remote_vertex_count = self.remote_vertices.len() as u32;
        if self.remote_vertex_count > 0 {
            self.queue.write_buffer(
                &self.player_buffer,
                0,
                bytemuck::cast_slice(&self.remote_vertices),
            );
        }
        self.remote_geometry_dirty = false;
    }

    fn queue_heartbeat(&mut self, now: Instant) {
        if self.connection.is_none() || self.connection_state != ConnectionState::Connected {
            return;
        }
        if let Some(sent_at) = self.pending_ping_sent_at {
            if now.duration_since(sent_at) >= HEARTBEAT_TIMEOUT {
                self.fail_connection("server heartbeat timed out; you can reconnect");
            }
            return;
        }
        if now.duration_since(self.last_heartbeat_sent_at) < HEARTBEAT_INTERVAL {
            return;
        }
        if let Ok(frame) = encode_client(&ClientMessage::Ping) {
            self.queue_network_command(&frame);
            if self.connection.is_some() {
                self.last_heartbeat_sent_at = now;
                self.pending_ping_sent_at = Some(now);
            }
        }
    }

    fn queue_fire(&mut self) {
        if self.connection.is_none() || self.connection_state != ConnectionState::Connected {
            return;
        }
        let now = Instant::now();
        if self
            .last_shot_sent_at
            .is_some_and(|last_shot| now.saturating_duration_since(last_shot) < SHOT_LOCAL_COOLDOWN)
        {
            return;
        }
        let direction = glam::Vec3::new(
            self.yaw.cos() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.sin() * self.pitch.cos(),
        )
        .normalize()
        .to_array();
        match encode_client(&ClientMessage::Fire { direction }) {
            Ok(frame) => {
                self.queue_network_command(&frame);
                if self.connection.is_some() {
                    self.last_shot_sent_at = Some(now);
                }
                if self.console.open {
                    self.console.push_line("shot fired");
                }
            }
            Err(error) if self.console.open => self.console.push_error(error.to_string()),
            Err(_) => {}
        }
    }

    fn execute_console_input(&mut self) -> bool {
        let input = self.console.take_input();
        let command = match parse_command(&input) {
            Ok(Some(command)) => command,
            Ok(None) => return false,
            Err(error) => {
                self.console.push_error(error);
                return false;
            }
        };
        match command {
            ClientCommand::Help => {
                self.console
                    .push_line("help | connect HOST:PORT | disconnect | quit");
                self.console.push_line(
                    "fps show | net stats | ping | say TEXT | name NAME | state | clear",
                );
                self.console
                    .push_line("F1 closes the console. Arrow keys browse command history.");
                self.console
                    .push_line("Lock the mouse and left-click to fire the hitscan weapon.");
            }
            ClientCommand::Connect(address) => {
                if self.connection_state != ConnectionState::Disconnected {
                    self.console
                        .push_error("disconnect or wait for the current connection attempt first");
                } else {
                    match connect_game(&address, self.profile_id) {
                        Ok((stream, since)) => {
                            self.connection = Some(stream);
                            self.connection_state = ConnectionState::AwaitingWelcome { since };
                            self.last_heartbeat_sent_at = Instant::now();
                            self.receive_buffer.clear();
                            self.send_buffer.clear();
                            self.send_offset = 0;
                            self.remote_vertex_count = 0;
                            self.remote_player_count = 0;
                            self.remote_players = [None; MAX_VISIBLE_PLAYERS];
                            self.last_remote_snapshot_time_ms = None;
                            self.last_remote_snapshot_received_at = None;
                            self.remote_geometry_dirty = false;
                            self.health = PLAYER_MAX_HEALTH;
                            self.console
                                .push_line(format!("awaiting welcome from {address}"));
                        }
                        Err(error) => self.console.push_error(format!("connect failed: {error}")),
                    }
                }
            }
            ClientCommand::Disconnect => {
                self.clear_connection();
                self.console.push_line("disconnected");
            }
            ClientCommand::FpsShow => self.console.push_line(self.frame_stats.summary()),
            ClientCommand::NetStats => {
                let round_trip = self.round_trip_ms.map_or_else(
                    || "n/a".to_owned(),
                    |milliseconds| format!("{milliseconds} ms"),
                );
                self.console.push_line(format!(
                    "tx={} bytes rx={} bytes rtt={} nearby_players={}",
                    self.bytes_sent, self.bytes_received, round_trip, self.remote_player_count
                ));
            }
            ClientCommand::Ping => self.queue_console_message(ClientMessage::Ping, "ping sent"),
            ClientCommand::Chat(text) => {
                self.queue_console_message(ClientMessage::Chat(text), "chat sent")
            }
            ClientCommand::SetName(name) => {
                self.queue_console_message(ClientMessage::SetName(name), "name sent")
            }
            ClientCommand::State => self.console.push_line(format!(
                "position=({:.2},{:.2},{:.2}) health={} HP nearby_players={}",
                self.character.position[0],
                self.character.position[1],
                self.character.position[2],
                self.health,
                self.remote_player_count
            )),
            ClientCommand::Clear => self.console.clear_lines(),
            ClientCommand::Quit => return true,
        }
        false
    }

    fn queue_console_message(&mut self, message: ClientMessage, description: &str) {
        if self.connection_state != ConnectionState::Connected || self.connection.is_none() {
            if matches!(
                self.connection_state,
                ConnectionState::AwaitingWelcome { .. }
            ) {
                self.console.push_error("waiting for server welcome");
            } else {
                self.console.push_error("not connected to a server");
            }
            return;
        }
        let is_ping = matches!(&message, ClientMessage::Ping);
        if is_ping && self.pending_ping_sent_at.is_some() {
            self.console
                .push_error("a ping is already waiting for a response");
            return;
        }
        match encode_client(&message) {
            Ok(frame) => {
                let sent_at = Instant::now();
                self.queue_network_command(&frame);
                if is_ping && self.connection.is_some() {
                    self.last_heartbeat_sent_at = sent_at;
                    self.pending_ping_sent_at = Some(sent_at);
                }
                if self.connection.is_some() {
                    self.console.push_line(description);
                }
            }
            Err(error) => self.console.push_error(error.to_string()),
        }
    }

    fn queue_network_command(&mut self, command: &[u8]) {
        if self.connection.is_none() || self.connection_state != ConnectionState::Connected {
            return;
        }
        if self.send_offset > 0 {
            self.send_buffer.drain(..self.send_offset);
            self.send_offset = 0;
        }
        if self.send_buffer.len() + command.len() > MAX_NETWORK_FRAME_BYTES {
            self.fail_connection("outgoing server frame exceeds protocol limits");
            return;
        }
        self.send_buffer.extend_from_slice(command);
    }

    fn flush_network_commands(&mut self) {
        if self.connection_state != ConnectionState::Connected {
            self.send_buffer.clear();
            self.send_offset = 0;
            return;
        }
        let Some(stream) = self.connection.as_mut() else {
            self.clear_connection();
            return;
        };
        let mut disconnected = false;
        while self.send_offset < self.send_buffer.len() {
            match stream.write(&self.send_buffer[self.send_offset..]) {
                Ok(0) => break,
                Ok(count) => {
                    self.send_offset += count;
                    self.bytes_sent = self.bytes_sent.saturating_add(count as u64);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected {
            self.fail_connection("connection lost while sending; you can reconnect");
        } else if self.send_offset == self.send_buffer.len() {
            self.send_buffer.clear();
            self.send_offset = 0;
        }
    }
}

fn create_ui_resources(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::BindGroup, wgpu::Buffer) {
    let atlas = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("minimium-console-font-atlas"),
        size: wgpu::Extent3d {
            width: 128,
            height: 64,
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
    let mut upload = vec![0; 256 * 64];
    for row in 0..64 {
        upload[row * 256..row * 256 + 128].copy_from_slice(&font[row * 128..row * 128 + 128]);
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
            bytes_per_row: Some(256),
            rows_per_image: Some(64),
        },
        wgpu::Extent3d {
            width: 128,
            height: 64,
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
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(UI_SHADER)),
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

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    connection: Option<(TcpStream, Instant)>,
    world_map: Option<WorldMap>,
    profile_id: PlayerProfileId,
    startup_error: Option<String>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = WindowAttributes::default()
            .with_title("Minimium 3D client")
            .with_inner_size(PhysicalSize::new(1280, 720));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.startup_error = Some(format!("window creation failed: {error}"));
                event_loop.exit();
                return;
            }
        };
        let world_map = self
            .world_map
            .take()
            .expect("renderer can only initialize once");
        match pollster::block_on(Renderer::new(
            Arc::clone(&window),
            self.connection.take(),
            world_map,
            self.profile_id,
        )) {
            Ok(renderer) => {
                self.window = Some(window);
                self.renderer = Some(renderer);
            }
            Err(error) => {
                self.startup_error = Some(format!("graphics initialization failed: {error}"));
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let (Some(window), Some(renderer)) = (&self.window, &mut self.renderer) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => renderer.resize(size),
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::F1 && event.state == ElementState::Pressed && !event.repeat
                    {
                        renderer.console.open = !renderer.console.open;
                        let _ = window.set_cursor_grab(CursorGrabMode::None);
                        window.set_cursor_visible(true);
                        renderer.cursor_locked = false;
                        renderer.last_cursor = None;
                        renderer.keys.clear();
                        renderer.jump_pending = false;
                        return;
                    }
                    if renderer.console.open {
                        if event.state == ElementState::Pressed {
                            match code {
                                KeyCode::Escape => renderer.console.open = false,
                                KeyCode::Enter => {
                                    if renderer.execute_console_input() {
                                        event_loop.exit();
                                    }
                                }
                                KeyCode::Backspace => renderer.console.backspace(),
                                KeyCode::ArrowUp => renderer.console.history_up(),
                                KeyCode::ArrowDown => renderer.console.history_down(),
                                KeyCode::Tab => renderer.console.complete_input(),
                                _ => {
                                    if let Some(text) = event.text.as_ref() {
                                        renderer.console.append_text(text.as_str());
                                    }
                                }
                            }
                        }
                        return;
                    }
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        let _ = window.set_cursor_grab(CursorGrabMode::None);
                        window.set_cursor_visible(true);
                        renderer.cursor_locked = false;
                        renderer.last_cursor = None;
                    } else if event.state == ElementState::Pressed {
                        renderer.keys.insert(code);
                        if code == KeyCode::Space && !event.repeat {
                            renderer.jump_pending = true;
                        }
                    } else {
                        renderer.keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if !renderer.console.open {
                    if renderer.cursor_locked {
                        renderer.queue_fire();
                    } else if window.set_cursor_grab(CursorGrabMode::Locked).is_ok()
                        || window.set_cursor_grab(CursorGrabMode::Confined).is_ok()
                    {
                        window.set_cursor_visible(false);
                        renderer.cursor_locked = true;
                        renderer.last_cursor = None;
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if renderer.cursor_locked && !renderer.console.open {
                    if let Some((x, y)) = renderer.last_cursor {
                        renderer.yaw += ((position.x - x) as f32) * 0.0025;
                        renderer.pitch -= ((position.y - y) as f32) * 0.0025;
                        renderer.pitch = renderer.pitch.clamp(-1.5, 1.5);
                    }
                }
                renderer.last_cursor = Some((position.x, position.y));
            }
            WindowEvent::RedrawRequested => {
                let started = Instant::now();
                let interval = started.saturating_duration_since(renderer.last_redraw);
                renderer.last_redraw = started;
                renderer.update();
                let draws = match renderer.render() {
                    Ok(draws) => draws,
                    Err(
                        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated,
                    ) => {
                        renderer.resize(window.inner_size());
                        0
                    }
                    Err(error) => {
                        eprintln!("render error: {error:?}");
                        0
                    }
                };
                renderer
                    .frame_stats
                    .record_frame(interval, started.elapsed(), draws);
                if draws > 0 {
                    renderer.fps_frames += 1;
                }
                let sample_time = renderer.fps_time.elapsed().as_secs_f64();
                if sample_time >= 1.0 {
                    let fps = renderer.fps_frames as f64 / sample_time;
                    renderer.fps_frames = 0;
                    renderer.fps_time = Instant::now();
                    window.set_title(&format!("Minimium | {fps:.1} FPS | {} HP | WASD move, Space jump, left click fire, Esc release", renderer.health));
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let DeviceEvent::MouseMotion { delta } = event {
            if let Some(renderer) = &mut self.renderer {
                if renderer.cursor_locked && !renderer.console.open {
                    renderer.yaw += (delta.0 as f32) * 0.0025;
                    renderer.pitch -= (delta.1 as f32) * 0.0025;
                    renderer.pitch = renderer.pitch.clamp(-1.5, 1.5);
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }
}

pub fn run(options: crate::options::ClientOptions) -> Result<(), Box<dyn Error>> {
    let map_path = options.map_path.clone();
    let map = if let Some(path) = map_path.as_deref() {
        WorldMap::load_file(path)
            .map_err(|error| format!("could not load map {}: {error}", path.display()))?
    } else {
        WorldMap::parse(SAMPLE_MAP).map_err(|error| format!("invalid bundled map: {error}"))?
    };
    let (profile_dir, data_dir) = options.paths(map.fingerprint());
    let profile_id = PlayerProfileId::load_or_create(&profile_dir)?;
    let mut local_server = None;
    let connection = if options.demo {
        None
    } else if let Some(address) = &options.connect {
        Some(connect_game(address, profile_id)?)
    } else {
        let (stream, server) =
            crate::bootstrap::connect_or_start(&data_dir, map_path.as_deref(), &options.threads)?;
        local_server = Some(server);
        Some(identify_game(stream, profile_id)?)
    };
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
        Instant::now() + Duration::from_millis(16),
    ));
    let mut app = App {
        window: None,
        renderer: None,
        connection,
        world_map: Some(map),
        profile_id,
        startup_error: None,
    };
    event_loop.run_app(&mut app)?;
    if let Some(mut server) = local_server.take() {
        server.shutdown()?;
    }
    if let Some(error) = app.startup_error {
        return Err(error.into());
    }
    Ok(())
}

fn connect_game(
    address: &str,
    profile_id: PlayerProfileId,
) -> Result<(TcpStream, Instant), Box<dyn Error>> {
    identify_game(crate::bootstrap::connect(address)?, profile_id)
}

fn identify_game(
    mut stream: TcpStream,
    profile_id: PlayerProfileId,
) -> Result<(TcpStream, Instant), Box<dyn Error>> {
    stream.set_nodelay(true)?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    stream.write_all(&encode_client(&ClientMessage::Identify(
        *profile_id.as_bytes(),
    ))?)?;
    let since = Instant::now();
    stream.set_nonblocking(true)?;
    Ok((stream, since))
}

fn map_geometry(map: &WorldMap) -> Vec<Vertex> {
    let mut out = Vec::with_capacity(map.boxes().len() * 36 + map.triangles().len() * 3);
    for map_box in map.boxes() {
        cube(&mut out, map_box.center, map_box.size, map_box.color);
    }
    for triangle in map.triangles() {
        for position in triangle.vertices {
            out.push(Vertex {
                position,
                color: triangle.color,
            });
        }
    }
    out
}

fn cube(out: &mut Vec<Vertex>, center: [f32; 3], size: [f32; 3], color: [f32; 3]) {
    let [cx, cy, cz] = center;
    let [sx, sy, sz] = [size[0] * 0.5, size[1] * 0.5, size[2] * 0.5];
    let p = [
        [cx - sx, cy - sy, cz - sz],
        [cx + sx, cy - sy, cz - sz],
        [cx + sx, cy + sy, cz - sz],
        [cx - sx, cy + sy, cz - sz],
        [cx - sx, cy - sy, cz + sz],
        [cx + sx, cy - sy, cz + sz],
        [cx + sx, cy + sy, cz + sz],
        [cx - sx, cy + sy, cz + sz],
    ];
    for face in [
        [0, 1, 2, 0, 2, 3],
        [5, 4, 7, 5, 7, 6],
        [4, 0, 3, 4, 3, 7],
        [1, 5, 6, 1, 6, 2],
        [3, 2, 6, 3, 6, 7],
        [4, 5, 1, 4, 1, 0],
    ] {
        for index in face {
            out.push(Vertex {
                position: p[index],
                color,
            });
        }
    }
}
