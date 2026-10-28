use crate::console::{font_atlas_pixels, parse_command, ClientCommand, Console, UiVertex};
use minimium_core::{
    endpoint::ServerEndpoint,
    identity::PlayerProfileId,
    world::{
        step_character, CharacterInput, CharacterState, CollisionIndex, CollisionQuery, WorldMap,
    },
};
use minimium_protocol::{decode_server, encode_client, ClientMessage, ServerMessage};
use std::{
    borrow::Cow,
    collections::HashSet,
    error::Error,
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    thread,
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
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    size: PhysicalSize<u32>,
    character: CharacterState,
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
    display_fps: u32,
    bytes_sent: u64,
    bytes_received: u64,
    remote_player_count: usize,
    map_fingerprint: u64,
    profile_id: PlayerProfileId,
    connection: Option<TcpStream>,
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
        connection: Option<TcpStream>,
        map: WorldMap,
        profile_id: PlayerProfileId,
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
            camera_buffer,
            camera_bind_group,
            depth_view,
            size,
            character: CharacterState::at_spawn(map.spawn),
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
            display_fps: 0,
            bytes_sent: 0,
            bytes_received: 0,
            remote_player_count: 0,
            map_fingerprint,
            profile_id,
            connection,
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
    }

    fn update(&mut self, window: &Window) {
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
            if self.connection.is_some()
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
        self.flush_network_commands();
        self.receive_network_state();

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

        self.fps_frames += 1;
        if now.duration_since(self.fps_time).as_secs_f32() >= 1.0 {
            let fps = self.fps_frames;
            self.display_fps = fps;
            self.fps_frames = 0;
            self.fps_time = now;
            window.set_title(&format!(
                "Minimium | {fps} FPS | WASD move, Space jump, mouse look, Esc release"
            ));
        }
        let vertex_count = {
            let vertices = self
                .console
                .build_vertices(self.size.width, self.size.height);
            let count = vertices.len() as u32;
            if count > 0 {
                self.queue
                    .write_buffer(&self.ui_vertex_buffer, 0, bytemuck::cast_slice(vertices));
            }
            count
        };
        self.ui_vertex_count = vertex_count;
    }

    fn render(&mut self) -> Result<(), wgpu::CurrentSurfaceTexture> {
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                self.surface.configure(&self.device, &self.config);
                frame
            }
            error @ (wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost) => {
                return Err(error)
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(())
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
            if self.remote_vertex_count > 0 {
                pass.set_vertex_buffer(0, self.player_buffer.slice(..));
                pass.draw(0..self.remote_vertex_count, 0..1);
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
        }
        self.queue.submit(Some(encoder.finish()));
        self.queue.present(frame);
        Ok(())
    }

    fn receive_network_state(&mut self) {
        let Some(stream) = self.connection.as_mut() else {
            return;
        };
        let mut chunk = [0u8; 4096];
        let mut total_read = 0;
        loop {
            let available = (MAX_NETWORK_BYTES_PER_FRAME - total_read).min(chunk.len());
            if available == 0 {
                break;
            }
            match stream.read(&mut chunk[..available]) {
                Ok(0) => {
                    self.connection = None;
                    self.remote_vertex_count = 0;
                    self.remote_player_count = 0;
                    self.remote_vertices.clear();
                    return;
                }
                Ok(count) => {
                    total_read += count;
                    self.bytes_received = self.bytes_received.saturating_add(count as u64);
                    self.receive_buffer.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.connection = None;
                    self.remote_vertex_count = 0;
                    self.remote_player_count = 0;
                    self.remote_vertices.clear();
                    return;
                }
            }
        }
        if self.receive_buffer.len() > MAX_NETWORK_FRAME_BYTES {
            self.connection = None;
            self.receive_buffer.clear();
            self.remote_vertex_count = 0;
            self.remote_player_count = 0;
            self.remote_vertices.clear();
            return;
        }
        loop {
            let decoded = decode_server(&self.receive_buffer);
            match decoded {
                Ok(Some((message, frame_bytes))) => {
                    self.receive_buffer.drain(..frame_bytes);
                    match message {
                        ServerMessage::Welcome {
                            id,
                            map_fingerprint,
                        } => {
                            if map_fingerprint != self.map_fingerprint {
                                let error = format!(
                                    "map mismatch: client={:016x} server={map_fingerprint:016x}",
                                    self.map_fingerprint
                                );
                                eprintln!("{error}");
                                self.console.push_error(error);
                                self.connection = None;
                                self.remote_vertex_count = 0;
                                self.remote_player_count = 0;
                                self.remote_vertices.clear();
                                return;
                            }
                            if self.console.open {
                                self.console.push_line(format!("connected as player {id}"));
                            }
                        }
                        ServerMessage::Pong if self.console.open => {
                            self.console.push_line("PONG");
                        }
                        ServerMessage::Accepted if self.console.open => {
                            self.console.push_line("server accepted command");
                        }
                        ServerMessage::Error(code) => {
                            self.console.push_error(format!("server error code {code}"));
                        }
                        ServerMessage::Bye if self.console.open => {
                            self.console.push_line("server closed the session");
                        }
                        ServerMessage::Snapshot {
                            self_position,
                            self_velocity,
                            self_grounded,
                            players,
                        } => {
                            self.remote_player_count = players.len();
                            let predicted = glam::Vec3::from_array(self.character.position);
                            let authoritative = glam::Vec3::from_array(self_position);
                            self.character.position =
                                predicted.lerp(authoritative, 0.25).to_array();
                            self.character.velocity = self_velocity;
                            self.character.grounded = self_grounded;
                            self.remote_vertices.clear();
                            for player in players.iter() {
                                cube(
                                    &mut self.remote_vertices,
                                    [
                                        player.position[0],
                                        player.position[1] + 0.85,
                                        player.position[2],
                                    ],
                                    [0.6, 1.7, 0.6],
                                    [0.82, 0.16, 0.12],
                                );
                            }
                            self.remote_vertex_count = self.remote_vertices.len() as u32;
                            if !self.remote_vertices.is_empty() {
                                self.queue.write_buffer(
                                    &self.player_buffer,
                                    0,
                                    bytemuck::cast_slice(&self.remote_vertices),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    eprintln!("invalid game protocol data: {error}");
                    self.connection = None;
                    self.receive_buffer.clear();
                    self.remote_vertex_count = 0;
                    self.remote_player_count = 0;
                    self.remote_vertices.clear();
                    return;
                }
            }
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
                self.console
                    .push_line("fps show | net stats | ping | say NAME | state | clear");
                self.console
                    .push_line("F1 closes the console. Arrow keys browse command history.");
            }
            ClientCommand::Connect(address) => match connect_game(&address, self.profile_id) {
                Ok(stream) => {
                    self.connection = Some(stream);
                    self.receive_buffer.clear();
                    self.send_buffer.clear();
                    self.send_offset = 0;
                    self.remote_vertex_count = 0;
                    self.remote_player_count = 0;
                    self.console.push_line(format!("connected to {address}"));
                }
                Err(error) => self.console.push_error(format!("connect failed: {error}")),
            },
            ClientCommand::Disconnect => {
                self.connection = None;
                self.receive_buffer.clear();
                self.send_buffer.clear();
                self.send_offset = 0;
                self.remote_vertex_count = 0;
                self.remote_player_count = 0;
                self.console.push_line("disconnected");
            }
            ClientCommand::FpsShow => self.console.push_line(format!("{} FPS", self.display_fps)),
            ClientCommand::NetStats => self.console.push_line(format!(
                "tx={} bytes rx={} bytes nearby_players={}",
                self.bytes_sent, self.bytes_received, self.remote_player_count
            )),
            ClientCommand::Ping => self.queue_console_message(ClientMessage::Ping, "ping sent"),
            ClientCommand::Say(name) => {
                self.queue_console_message(ClientMessage::SetName(name), "name sent")
            }
            ClientCommand::State => self.console.push_line(format!(
                "position=({:.2},{:.2},{:.2}) nearby_players={}",
                self.character.position[0],
                self.character.position[1],
                self.character.position[2],
                self.remote_player_count
            )),
            ClientCommand::Clear => self.console.clear_lines(),
            ClientCommand::Quit => return true,
        }
        false
    }

    fn queue_console_message(&mut self, message: ClientMessage, description: &str) {
        if self.connection.is_none() {
            self.console.push_error("not connected to a server");
            return;
        }
        match encode_client(&message) {
            Ok(frame) => {
                self.queue_network_command(&frame);
                self.console.push_line(description);
            }
            Err(error) => self.console.push_error(error.to_string()),
        }
    }

    fn queue_network_command(&mut self, command: &[u8]) {
        if self.send_offset > 0 {
            self.send_buffer.drain(..self.send_offset);
            self.send_offset = 0;
        }
        if self.send_buffer.len() + command.len() > MAX_NETWORK_FRAME_BYTES {
            self.connection = None;
            self.send_buffer.clear();
            return;
        }
        self.send_buffer.extend_from_slice(command);
    }

    fn flush_network_commands(&mut self) {
        let Some(stream) = self.connection.as_mut() else {
            self.send_buffer.clear();
            self.send_offset = 0;
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
            self.connection = None;
            self.send_buffer.clear();
            self.send_offset = 0;
            self.remote_vertex_count = 0;
            self.remote_player_count = 0;
            self.remote_vertices.clear();
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
    connection: Option<TcpStream>,
    world_map: Option<WorldMap>,
    profile_id: PlayerProfileId,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = WindowAttributes::default()
            .with_title("Minimium 3D client")
            .with_inner_size(PhysicalSize::new(1280, 720));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("window creation failed"),
        );
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
                eprintln!("graphics initialization failed: {error}");
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
                if !renderer.console.open
                    && (window.set_cursor_grab(CursorGrabMode::Locked).is_ok()
                        || window.set_cursor_grab(CursorGrabMode::Confined).is_ok())
                {
                    window.set_cursor_visible(false);
                    renderer.cursor_locked = true;
                    renderer.last_cursor = None;
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
                renderer.update(window);
                match renderer.render() {
                    Ok(()) => {}
                    Err(
                        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated,
                    ) => renderer.resize(window.inner_size()),
                    Err(error) => eprintln!("render error: {error:?}"),
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

pub fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut demo_only = false;
    let mut remote = None;
    let mut map_path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--demo" => demo_only = true,
            "--connect" => {
                index += 1;
                remote = Some(
                    args.get(index)
                        .ok_or("--connect requires HOST:PORT")?
                        .clone(),
                );
            }
            "--map" => {
                index += 1;
                map_path = Some(PathBuf::from(
                    args.get(index).ok_or("--map requires a path")?,
                ));
            }
            "--console" => {}
            other => return Err(format!("unknown client option `{other}` (try --help)").into()),
        }
        index += 1;
    }
    let map = if let Some(path) = map_path.as_deref() {
        WorldMap::load_file(path)
            .map_err(|error| format!("could not load map {}: {error}", path.display()))?
    } else {
        WorldMap::parse(SAMPLE_MAP).map_err(|error| format!("invalid bundled map: {error}"))?
    };
    let mut data_dir = PathBuf::from(".minimium-local");
    if map_path.is_some() {
        data_dir.push(format!("map-{:016x}", map.fingerprint()));
    }
    let profile_id = PlayerProfileId::load_or_create(&data_dir)?;
    let mut child = None;
    let connection = if demo_only {
        None
    } else if let Some(address) = remote {
        Some(connect_game(&address, profile_id)?)
    } else {
        Some(local_connection(
            &data_dir,
            &mut child,
            map_path.as_deref(),
            profile_id,
        )?)
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
    };
    event_loop.run_app(&mut app)?;
    if let Some(mut child) = child {
        if let Ok(endpoint) = ServerEndpoint::read(&data_dir) {
            let _ = stop_local_server(&endpoint);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if child.try_wait()?.is_some() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = child.kill();
    }
    Ok(())
}

fn connect_game(address: &str, profile_id: PlayerProfileId) -> Result<TcpStream, Box<dyn Error>> {
    let socket = address
        .to_socket_addrs()?
        .next()
        .ok_or("invalid server address")?;
    let mut stream = TcpStream::connect_timeout(&socket, Duration::from_secs(3))?;
    stream.set_nodelay(true)?;
    stream.write_all(&encode_client(&ClientMessage::Identify(
        *profile_id.as_bytes(),
    ))?)?;
    stream.set_nonblocking(true)?;
    Ok(stream)
}

fn local_connection(
    data_dir: &Path,
    child: &mut Option<Child>,
    map_path: Option<&Path>,
    profile_id: PlayerProfileId,
) -> Result<TcpStream, Box<dyn Error>> {
    if let Ok(endpoint) = ServerEndpoint::read(data_dir) {
        if let Ok(stream) = connect_game(&format!("127.0.0.1:{}", endpoint.game_port), profile_id) {
            return Ok(stream);
        }
    }
    let mut process = start_local_server(data_dir, map_path)?;
    let endpoint = match wait_endpoint(data_dir, &mut process) {
        Ok(endpoint) => endpoint,
        Err(error) => {
            let _ = process.kill();
            return Err(error);
        }
    };
    let stream = match connect_game(&format!("127.0.0.1:{}", endpoint.game_port), profile_id) {
        Ok(stream) => stream,
        Err(error) => {
            let _ = stop_local_server(&endpoint);
            let _ = process.kill();
            return Err(error);
        }
    };
    *child = Some(process);
    Ok(stream)
}

fn start_local_server(data_dir: &Path, map_path: Option<&Path>) -> Result<Child, Box<dyn Error>> {
    let current = std::env::current_exe()?;
    let name = if cfg!(windows) {
        "minimium-server.exe"
    } else {
        "minimium-server"
    };
    let path = current.with_file_name(name);
    if !path.exists() {
        return Err(format!(
            "{} is missing; build the workspace before starting the desktop client",
            path.display()
        )
        .into());
    }
    let mut command = Command::new(path);
    command
        .arg("run")
        .arg("--headless")
        .arg("--threads")
        .arg("1")
        .arg("--bind")
        .arg("127.0.0.1")
        .arg("--data-dir")
        .arg(data_dir);
    if let Some(map_path) = map_path {
        command.arg("--map").arg(map_path);
    }
    Ok(command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?)
}

fn wait_endpoint(
    data_dir: &std::path::Path,
    child: &mut Child,
) -> Result<ServerEndpoint, Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(8);
    while Instant::now() < deadline {
        if let Ok(endpoint) = ServerEndpoint::read(data_dir) {
            return Ok(endpoint);
        }
        if let Some(status) = child.try_wait()? {
            return Err(format!("local server exited during startup: {status}").into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Err("timed out waiting for local server".into())
}

fn stop_local_server(endpoint: &ServerEndpoint) -> Result<(), Box<dyn Error>> {
    let mut stream = TcpStream::connect(("127.0.0.1", endpoint.admin_port))?;
    writeln!(stream, "AUTH {}", endpoint.token)?;
    writeln!(stream, "stop")?;
    Ok(())
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
