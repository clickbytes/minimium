use crate::{
    client_commands::{self, CommandContext, CommandResult, FrontendAction},
    connection_controller::{ConnectionController, ConnectionEvent},
    connector::StartError,
    console::{Console, UiVertex},
    environment::{EnvironmentFrameInput, LocalDemoEnvironment},
    session::{ClientSession, DisconnectReason, SessionEvent, SessionState},
};
use minimium_core::{
    identity::PlayerProfileId,
    world::{
        character_frame, CharacterGravity, CharacterInput, CharacterState, CollisionIndex,
        CollisionQuery, GenerationRevision, SectorKey, WorldContentRevision, WorldManifest,
        WorldMap, WorldTopology, PLAYER_MAX_HEALTH,
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
mod celestial_halo_gpu;
mod celestial_labels;
mod client_game;
mod console_events;
mod controller;
mod diagnostics;
mod environment;
mod environment_gpu;
mod geometry;
mod globe;
mod globe_detail;
mod gpu;
mod gpu_surface;
mod init;
mod lighting;
mod moon_gpu;
mod moon_lighting;
mod moon_sphere_gpu;
mod output;
mod overview;
mod planet_visibility;
mod quality;
mod render;
mod scene_bridge;
mod sector_scene;
mod session;
mod shader_source;
mod shadow_casters;
mod shadow_gpu;
mod shadow_map_cache;
mod shadow_target;
mod shadows;
mod sky;
mod sky_gpu;
mod stars_gpu;
mod window;
mod window_metrics;
mod world_pipeline;

use environment::{EnvironmentFrame, RenderViewFrame, RenderViewInput};
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
    scene_color_format: wgpu::TextureFormat,
    hdr_enabled: bool,
    pipeline: wgpu::RenderPipeline,
    shadow_gpu: shadow_gpu::ShadowGpu,
    shadow_state: shadows::ShadowState,
    environment_pipelines: sky_gpu::EnvironmentPipelines,
    celestial_halo: celestial_halo_gpu::CelestialHaloGpu,
    stars: stars_gpu::StarGpu,
    moon: moon_gpu::MoonGpu,
    moon_sphere: moon_sphere_gpu::MoonSphereGpu,
    ui_pipeline: wgpu::RenderPipeline,
    ui_output_pipeline: Option<wgpu::RenderPipeline>,
    ui_bind_group: wgpu::BindGroup,
    ui_vertex_buffer: wgpu::Buffer,
    ui_vertex_count: u32,
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
    player_buffer: wgpu::Buffer,
    remote_vertex_count: u32,
    shadow_player_buffer: Option<wgpu::Buffer>,
    shadow_player_vertex_count: u32,
    shadow_remote_complete: bool,
    shadow_remote_dirty: bool,
    camera_buffer: wgpu::Buffer,
    environment_buffer: wgpu::Buffer,
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
    world_geometry_bounds:
        HashMap<(minimium_core::world::WorldInstanceId, SectorKey, u8), shadows::ShadowBounds>,
    active_sector_keys: Vec<(SectorKey, u8)>,
    active_world_instance: minimium_core::world::WorldInstanceId,
    render_origin: [f32; 3],
    shadow_map_buffer: Option<wgpu::Buffer>,
    shadow_map_instance: Option<minimium_core::world::WorldInstanceId>,
    shadow_map_fingerprint: Option<u64>,
    shadow_map_center: Option<[f64; 3]>,
    shadow_map_vertex_count: u32,
    overview_buffer: Option<(wgpu::Buffer, u32)>,
    overview_marker_buffer: Option<(wgpu::Buffer, u32)>,
    environment_draw_calls: sky::EnvironmentDrawCalls,
    last_shadow_prepare_cpu: Option<Duration>,
    last_environment_upload_cpu: Option<Duration>,
    overview_active: bool,
    overview_globe: bool,
    scene_visible: bool,
    globe: Option<globe::GlobeGpu>,
    output_encoder: Option<output::OutputEncoder>,
}

struct GameplayView {
    remote_vertices: Vec<Vertex>,
    remote_shadow_vertices: Vec<Vertex>,
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
    demo: bool,
}

struct Renderer {
    gpu: GpuResources,
    game: client_game::ClientGameState,
    input: InputState,
    ui: UiState,
    metrics: FrameMetrics,
    settings: ClientSettings,
    demo_universe_enabled: bool,
    fov_degrees: f32,
    show_celestial_names: bool,
    sky_quality: crate::environment::quality::SkyQualityState,
    connection: ConnectionController,
    environment_frame: EnvironmentFrameInput,
    render_environment_frame: EnvironmentFrame,
    environment_observer: environment::observer::ObserverAdapter,
    sky_calculator: minimium_astronomy::SkyCalculator,
    sky_catalogue: Option<environment::stars::StarCatalogueAdapter>,
    environment_error: Option<String>,
    demo_environment: Option<LocalDemoEnvironment>,
    world_catalog: std::collections::HashMap<minimium_core::world::WorldInstanceId, WorldMap>,
    sector_cache: sector_scene::ClientSectorCache,
    active_sector_keys: Vec<SectorKey>,
    last_render_position: [f32; 3],
    pending_sector_transitions:
        std::collections::HashMap<minimium_core::world::TransitionId, PendingSectorTransition>,
    overview_open: bool,
    overview_instance: Option<minimium_core::world::WorldInstanceId>,
    connection_status: String,
    globe_loading: Option<std::sync::mpsc::Receiver<Vec<u8>>>,
    globe_detail_loading: Option<std::sync::mpsc::Receiver<(globe_detail::Region, Vec<u8>)>>,
    overview_yaw: f32,
    overview_pitch: f32,
    overview_zoom: f32,
    overview_dragging: bool,
    overview_slider_dragging: bool,
    overview_dragged: bool,
    overview_drag_distance: f64,
    map_teleport_armed: bool,
    map_teleport_pending: bool,
    teleport_loading: bool,
}

#[derive(Clone, Copy)]
struct PendingSectorTransition {
    source_context: minimium_protocol::WorldContext,
    context: minimium_protocol::WorldContext,
    destination: [f32; 3],
    manifest: WorldManifest,
    ready_sent: bool,
}

pub use window::run;
