use crate::{
    client_commands::{self, CommandContext, CommandResult, FrontendAction},
    console::Console,
    environment::{EnvironmentFrameInput, LocalDemoEnvironment},
    session::{ClientSession, DisconnectReason, SessionEvent, SessionState},
};
use minimium_client_runtime::{connector::StartError, ConnectionController, ConnectionEvent};
use minimium_core::identity::PlayerProfileId;
use minimium_protocol::{ClientMessage, ServerMessage, MAX_VISIBLE_PLAYERS};
use minimium_simulation::{
    CharacterInput, CharacterState, CollisionIndex, CollisionQuery, PLAYER_MAX_HEALTH,
};
use minimium_world::world::{
    character_frame, CharacterGravity, SectorKey, WorldManifest, WorldMap, WorldTopology,
};
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
    window::{CursorGrabMode, Window, WindowId},
};

mod atmosphere_gpu;
mod camera;
mod celestial_labels;
mod client_game;
mod clouds;
mod console_events;
mod controller;
mod diagnostics;
mod environment;
mod environment_gpu;
mod gpu;
mod gpu_surface;
mod init;
mod materials;
mod overview;
mod quality;
mod render;
mod scene_bridge;
mod sector_scene;
mod sector_worker;
mod session;
mod shadow_gpu;
mod shadow_map_cache;
mod shadows;
mod sky;
mod terrain_status;
mod window;
mod window_events;
mod window_metrics;
mod window_mode;

use environment::{EnvironmentFrame, RenderViewFrame, RenderViewInput};
use minimium_renderer::geometry::{cube, sector_geometry, Vertex};

const UI_VERTEX_CAPACITY: usize = 8192;
const REMOTE_INTERPOLATION_DELAY: Duration = Duration::from_millis(100);
const SHOT_LOCAL_COOLDOWN: Duration = Duration::from_millis(300);

pub(super) fn validate_world_manifest(
    manifest: minimium_world::world::WorldManifest,
) -> Result<(), std::io::Error> {
    minimium_procgen::validate_world_manifest(manifest)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn legacy_manifest(id: u128, map: &WorldMap) -> minimium_world::world::WorldManifest {
    use minimium_world::world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, WorldContentRevision,
        WorldDefinitionId, WorldManifest, WorldSeed, WorldTopology,
    };
    let generation = GenerationSpec::new(
        WorldSeed::new(0),
        GenerationRevision::new(1).expect("legacy generator revision is nonzero"),
        1,
    )
    .expect("legacy position quantum is nonzero");
    WorldManifest::new(
        WorldDefinitionId::new(id).expect("legacy definition ID is nonzero"),
        WorldTopology::Dungeon,
        generation,
        WorldContentRevision::new(1).expect("legacy content revision is nonzero"),
        GeneratorDigest::legacy_map_fingerprint(map.fingerprint()),
    )
}

struct GpuResources {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    scene_color_format: wgpu::TextureFormat,
    hdr_enabled: bool,
    pipeline: wgpu::RenderPipeline,
    terrain_gpu: minimium_renderer::terrain_gpu::TerrainGpu,
    frame_layout: wgpu::BindGroupLayout,
    shadow_gpu: minimium_renderer::ShadowGpu,
    shadow_state: shadows::ShadowState,
    environment_pipelines: minimium_renderer::EnvironmentPipelines,
    milky_way: minimium_renderer::MilkyWayGpu,
    celestial_halo: minimium_renderer::CelestialHaloGpu,
    stars: minimium_renderer::StarGpu,
    uploaded_star_stamp: Option<environment::StarFrameStamp>,
    moon: minimium_renderer::MoonGpu,
    moon_sphere: minimium_renderer::MoonSphereGpu,
    atmosphere_gpu: atmosphere_gpu::AtmosphereGpu,
    cloud_gpu: clouds::CloudGpu,
    ui: minimium_renderer::ui_pipeline::UiGpuResources,
    initial_world: minimium_renderer::geometry_gpu::StaticVertexBuffer,
    remote_vertices: minimium_renderer::geometry_gpu::DynamicVertexBuffer,
    shadow_remote_complete: bool,
    shadow_remote_dirty: bool,
    camera_buffer: wgpu::Buffer,
    environment_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    size: PhysicalSize<u32>,
    fixed_gpu_bytes: u64,
    world_geometry: minimium_renderer::world_geometry_gpu::WorldGeometryGpu,
    active_sector_keys: Vec<(SectorKey, u8)>,
    active_world_instance: minimium_world::world::WorldInstanceId,
    render_origin: [f32; 3],
    shadow_map_instance: Option<minimium_world::world::WorldInstanceId>,
    shadow_map_fingerprint: Option<u64>,
    shadow_map_center: Option<[f64; 3]>,
    shadow_map_radius: Option<f32>,
    overview_buffers: minimium_renderer::overview_gpu::OverviewGpuBuffers,
    environment_draw_calls: sky::EnvironmentDrawCalls,
    last_shadow_prepare_cpu: Option<Duration>,
    last_environment_upload_cpu: Option<Duration>,
    overview_active: bool,
    overview_planet: bool,
    scene_visible: bool,
    output_encoder: Option<minimium_renderer::OutputEncoder>,
}

struct GameplayView {
    remote_vertices: Vec<Vertex>,
    remote_shadow_vertices: Vec<Vertex>,
    reconciliation: minimium_client_runtime::SnapshotReconciler,
    remote_positions_mm: Vec<[i64; 3]>,
    remote_geometry_dirty: bool,
    prediction: minimium_client_runtime::ClientPrediction,
    health: u16,
    collision_index: CollisionIndex,
    collision_query: CollisionQuery,
    last_shot_sent_at: Option<Instant>,
    hit_confirmation_until: Option<Instant>,
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
    sky_quality: minimium_renderer::quality::SkyQualityState,
    connection: ConnectionController,
    environment_frame: EnvironmentFrameInput,
    render_environment_frame: EnvironmentFrame,
    environment_observer: environment::observer::ObserverAdapter,
    sky_calculator: minimium_astronomy::SkyCalculator,
    sky_catalogue: Option<minimium_renderer::stars::StarCatalogueAdapter>,
    environment_error: Option<String>,
    weather: clouds::WeatherState,
    demo_environment: Option<LocalDemoEnvironment>,
    world_catalog: std::collections::HashMap<minimium_world::world::WorldInstanceId, WorldMap>,
    sector_cache: sector_scene::ClientSectorCache,
    active_sector_keys: Vec<SectorKey>,
    last_render_position: [f32; 3],
    pending_sector_transitions:
        std::collections::HashMap<minimium_world::world::TransitionId, PendingSectorTransition>,
    overview_open: bool,
    overview_instance: Option<minimium_world::world::WorldInstanceId>,
    connection_status: String,
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

mod collision_worker;
