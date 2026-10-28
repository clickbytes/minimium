use crate::{
    client_commands::{self, CommandContext, CommandResult, FrontendAction},
    connection_controller::{ConnectionController, ConnectionEvent},
    connector::StartError,
    console::{Console, UiVertex},
    session::{ClientSession, DisconnectReason, SessionEvent, SessionState},
    SAMPLE_MAP,
};
use minimium_core::{
    identity::PlayerProfileId,
    world::{
        step_character, CharacterInput, CharacterState, CollisionIndex, CollisionQuery, WorldMap,
        PLAYER_MAX_HEALTH,
    },
};
use minimium_protocol::{ClientMessage, ServerMessage, MAX_VISIBLE_PLAYERS};
use std::{
    collections::HashSet,
    error::Error,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{CursorGrabMode, Window, WindowAttributes, WindowId},
};

mod camera;
mod controller;
mod geometry;
mod gpu;
mod init;
mod render;
mod session;
mod window;

use geometry::{cube, map_geometry, Vertex};

const UI_VERTEX_CAPACITY: usize = 8192;
const REMOTE_INTERPOLATION_DELAY: Duration = Duration::from_millis(100);
const SHOT_LOCAL_COOLDOWN: Duration = Duration::from_millis(300);

#[derive(Clone, Copy)]
pub(super) struct RemotePlayerTrack {
    pub(super) id: minimium_core::ids::PlayerId,
    pub(super) previous_position: [f32; 3],
    pub(super) current_position: [f32; 3],
    pub(super) displayed_position: [f32; 3],
    pub(super) previous_time_ms: u64,
    pub(super) current_time_ms: u64,
    pub(super) displayed_time_ms: u64,
}
struct GpuResources {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    ui_pipeline: wgpu::RenderPipeline,
    ui_bind_group: wgpu::BindGroup,
    ui_vertex_buffer: wgpu::Buffer,
    ui_vertex_count: u32,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    player_buffer: wgpu::Buffer,
    remote_vertex_count: u32,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    size: PhysicalSize<u32>,
    fixed_gpu_bytes: u64,
}

struct GameplayView {
    remote_vertices: Vec<Vertex>,
    remote_players: [Option<RemotePlayerTrack>; MAX_VISIBLE_PLAYERS],
    last_remote_snapshot_time_ms: Option<u64>,
    last_remote_snapshot_received_at: Option<Instant>,
    remote_geometry_dirty: bool,
    character: CharacterState,
    health: u16,
    collision_index: CollisionIndex,
    collision_query: CollisionQuery,
    remote_player_count: usize,
    last_shot_sent_at: Option<Instant>,
    hit_confirmation_until: Option<Instant>,
    physics_accumulator: f32,
}

struct InputState {
    yaw: f32,
    pitch: f32,
    keys: HashSet<KeyCode>,
    last_cursor: Option<(f64, f64)>,
    cursor_locked: bool,
    jump_pending: bool,
    input_keepalive: f32,
    last_sent_direction: [f32; 2],
}

struct UiState {
    console: Console,
    session_events: Vec<SessionEvent>,
}

struct FrameMetrics {
    last_frame: Instant,
    fps_time: Instant,
    fps_frames: u32,
    frame_stats: crate::stats::FrameStats,
    last_redraw: Instant,
    previous_bytes_sent: u64,
    previous_bytes_received: u64,
}

struct ClientSettings {
    data_dir: PathBuf,
    map_path: Option<PathBuf>,
    threads: crate::options::ThreadLane,
}

struct Renderer {
    gpu: GpuResources,
    game: GameplayView,
    input: InputState,
    ui: UiState,
    metrics: FrameMetrics,
    settings: ClientSettings,
    connection: ConnectionController,
}

pub use window::run;
