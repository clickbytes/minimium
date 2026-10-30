use crate::{
    client_commands::{self, CommandContext, CommandResult, FrontendAction},
    connection_controller::{ConnectionController, ConnectionEvent},
    connector::StartError,
    console::{Console, UiVertex},
    session::{ClientSession, DisconnectReason, SessionEvent, SessionState},
};
use minimium_core::{
    identity::PlayerProfileId,
    world::{
        character_frame, step_character, CharacterGravity, CharacterInput, CharacterState,
        CollisionIndex, CollisionQuery, GenerationRevision, SectorKey, WorldContentRevision,
        WorldManifest, WorldMap, WorldTopology, PLAYER_MAX_HEALTH,
    },
};
use minimium_protocol::{ClientMessage, ServerMessage, MAX_VISIBLE_PLAYERS};
use std::{
    collections::{HashMap, HashSet},
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
mod console_events;
mod controller;
mod geometry;
mod globe;
mod gpu;
mod init;
mod overview;
mod planet_visibility;
mod render;
mod sector_scene;
mod session;
mod window;

use geometry::{cube, sector_geometry, Vertex};

const UI_VERTEX_CAPACITY: usize = 8192;
const REMOTE_INTERPOLATION_DELAY: Duration = Duration::from_millis(100);
const SHOT_LOCAL_COOLDOWN: Duration = Duration::from_millis(300);

#[derive(Clone, Copy)]
pub(super) struct RemotePlayerTrack {
    pub(super) id: minimium_core::ids::PlayerId,
    pub(super) previous_position_mm: [i64; 3],
    pub(super) current_position_mm: [i64; 3],
    pub(super) displayed_position_mm: [i64; 3],
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
    world_vertex_catalog: std::collections::HashMap<
        (minimium_core::world::WorldInstanceId, SectorKey, u8),
        (wgpu::Buffer, u32),
    >,
    world_sector_versions:
        HashMap<minimium_core::world::WorldInstanceId, (GenerationRevision, WorldContentRevision)>,
    active_sector_keys: Vec<(SectorKey, u8)>,
    active_world_instance: minimium_core::world::WorldInstanceId,
    render_origin: [f32; 3],
    overview_buffer: Option<(wgpu::Buffer, u32)>,
    overview_active: bool,
    overview_globe: bool,
    scene_visible: bool,
    globe: Option<globe::GlobeGpu>,
}

struct GameplayView {
    remote_vertices: Vec<Vertex>,
    remote_players: [Option<RemotePlayerTrack>; MAX_VISIBLE_PLAYERS],
    last_remote_snapshot_time_ms: Option<u64>,
    last_remote_snapshot_received_at: Option<Instant>,
    remote_geometry_dirty: bool,
    character: CharacterState,
    position_mm: Option<minimium_core::world::Position3Mm>,
    health: u16,
    collision_index: CollisionIndex,
    collision_query: CollisionQuery,
    remote_player_count: usize,
    last_shot_sent_at: Option<Instant>,
    hit_confirmation_until: Option<Instant>,
    physics_accumulator: f32,
    sectors_ready: bool,
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
    world_definition_path: Option<PathBuf>,
    threads: crate::options::ThreadLane,
    require_earth: bool,
}

struct Renderer {
    gpu: GpuResources,
    game: GameplayView,
    input: InputState,
    ui: UiState,
    metrics: FrameMetrics,
    settings: ClientSettings,
    connection: ConnectionController,
    world_catalog: std::collections::HashMap<minimium_core::world::WorldInstanceId, WorldMap>,
    world_manifests:
        std::collections::HashMap<minimium_core::world::WorldInstanceId, WorldManifest>,
    sector_cache: sector_scene::ClientSectorCache,
    active_sector_keys: Vec<SectorKey>,
    last_render_position: [f32; 3],
    pending_sector_transitions:
        std::collections::HashMap<minimium_core::world::TransitionId, PendingSectorTransition>,
    current_instance: minimium_core::world::WorldInstanceId,
    overview_open: bool,
    overview_instance: Option<minimium_core::world::WorldInstanceId>,
    connection_status: String,
    globe_loading: Option<std::sync::mpsc::Receiver<Vec<u8>>>,
    overview_yaw: f32,
    overview_pitch: f32,
    overview_zoom: f32,
    overview_dragging: bool,
    overview_dragged: bool,
    overview_drag_distance: f64,
    map_teleport_armed: bool,
    map_teleport_pending: bool,
    teleport_loading: bool,
}

#[derive(Clone, Copy)]
struct PendingSectorTransition {
    context: minimium_protocol::WorldContext,
    destination: [f32; 3],
    manifest: WorldManifest,
}

pub use window::run;
