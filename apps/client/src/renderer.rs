use crate::application::PendingSectorTransition;
use crate::{
    client_commands::{self, CommandContext, CommandResult, FrontendAction},
    console::Console,
    environment::{EnvironmentFrameInput, LocalDemoEnvironment},
    session::{ClientSession, DisconnectReason, SessionEvent, SessionState},
};
use minimium_client_runtime::{ConnectionEvent, connector::StartError};
use minimium_core::identity::PlayerProfileId;
use minimium_protocol::{MAX_VISIBLE_PLAYERS, ServerMessage};
use minimium_simulation::{CharacterState, CollisionQuery, MovementIntent, PLAYER_MAX_HEALTH};
use minimium_world::world::{
    CharacterGravity, WorldManifest, WorldMap, WorldTopology, character_frame,
};
use std::{
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
mod bolids;
mod camera;
mod celestial_labels;
mod clouds;
mod console_events;
mod controller;
mod dev_input;
mod diagnostics;
mod environment;
mod environment_gpu;
mod gameplay_presentation;
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
mod settings;
mod shadow_gpu;
mod shadow_map_cache;
mod shadow_prepare;
mod shadows;
mod sky;
mod terrain_status;
mod window;
mod window_events;
mod window_metrics;
mod window_mode;

use environment::{EnvironmentFrame, RenderViewFrame, RenderViewInput};
pub(crate) use minimium_renderer::geometry::Vertex;
use minimium_renderer::geometry::{cube, sector_geometry};

const UI_VERTEX_CAPACITY: usize = 8192;
pub(crate) const REMOTE_INTERPOLATION_DELAY: Duration = Duration::from_millis(100);
const SHOT_LOCAL_COOLDOWN: Duration = Duration::from_millis(300);

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
    shadow_remote_stamp: Option<minimium_renderer::world_geometry_gpu::GeometryPublicationStamp>,
    shadow_remote_dirty: bool,
    shadow_publish_ready: bool,
    shadow_upload_status: shadow_gpu::ShadowUploadStatus,
    camera_buffer: wgpu::Buffer,
    environment_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    depth_view: wgpu::TextureView,
    size: PhysicalSize<u32>,
    fixed_gpu_bytes: u64,
    world_geometry: minimium_renderer::world_geometry_gpu::WorldGeometryGpu,
    render_origin: [f32; 3],
    shadow_map_key: Option<shadow_map_cache::ShadowMapCacheKey>,
    overview_buffers: minimium_renderer::overview_gpu::OverviewGpuBuffers,
    environment_draw_calls: sky::EnvironmentDrawCalls,
    meteors_status: minimium_renderer::meteors::MeteorRenderStatus,
    last_shadow_prepare_cpu: Option<Duration>,
    last_environment_upload_cpu: Option<Duration>,
    overview_active: bool,
    overview_planet: bool,
    scene_visible: bool,
    output_encoder: Option<minimium_renderer::OutputEncoder>,
}

struct GameplayPresentation {
    pub(crate) remote_vertices: Vec<Vertex>,
    pub(crate) remote_shadow_vertices: Vec<Vertex>,
    pub(crate) remote_geometry_dirty: bool,
    pub(crate) last_shot_sent_at: Option<Instant>,
    pub(crate) hit_confirmation_until: Option<Instant>,
}

struct InputState {
    yaw: f32,
    pitch: f32,
    last_cursor: Option<(f64, f64)>,
    cursor_locked: bool,
    jump_pending: bool,
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

struct Renderer {
    prepared_bolids: minimium_renderer::milky_way_gpu::BolidGpuFrame,
    diagnostic_bolid_age: Option<(minimium_protocol::WorldContext, u64, f32)>,
    bolids: Vec<(
        minimium_protocol::WorldContext,
        minimium_protocol::BolidEvent,
        Instant,
        Option<minimium_renderer::milky_way_gpu::BolidEvolution>,
    )>,
    last_sky_view_at: Option<Instant>,
    pending_bolid_request: bool,
    localizer: minimium_localization::Localizer,
    gpu: GpuResources,
    shadow_state: shadows::ShadowState,
    prepared_shadow: shadow_prepare::PreparedShadowFrame,
    shadow_map_preparation: shadow_map_cache::ShadowMapPreparationCache,
    input: InputState,
    metrics: FrameMetrics,
    demo_universe_enabled: bool,
    fov_degrees: f32,
    mouse_sensitivity: f32,
    show_celestial_names: bool,
    atmosphere_debug: minimium_renderer::atmosphere_debug::AtmosphereDebug,
    terrain_debug: minimium_renderer::terrain_debug::TerrainDebug,
    sky_quality: minimium_renderer::quality::SkyQualityState,
    environment_frame: EnvironmentFrameInput,
    render_environment_frame: EnvironmentFrame,
    environment_observer: environment::observer::ObserverAdapter,
    sky_calculator: minimium_astronomy::SkyCalculator,
    sky_catalogue: Option<minimium_renderer::stars::StarCatalogueAdapter>,
    environment_error: Option<String>,
    weather: clouds::WeatherState,
    demo_environment: Option<LocalDemoEnvironment>,
    application: crate::application::ClientApplication,
    gameplay_presentation: GameplayPresentation,
    sector_cache: sector_scene::ClientSectorCache,
    last_render_position: [f32; 3],
}

pub use window::run;

mod collision_worker;
