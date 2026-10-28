use crate::{
    bootstrap::LocalServer,
    client_commands::{self, CommandContext, CommandResult, FrontendAction},
    connector::{AttemptId, Connector, PollEvent, StartError},
    console::{font_atlas_pixels, Console, UiVertex},
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
mod render;
mod session;
mod window;

use geometry::{cube, map_geometry, Vertex};

const UI_VERTEX_CAPACITY: usize = 8192;
const REMOTE_INTERPOLATION_DELAY: Duration = Duration::from_millis(100);
const SHOT_LOCAL_COOLDOWN: Duration = Duration::from_millis(300);
const CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy)]
pub(super) struct RemotePlayerTrack {
    pub(super) id: u64,
    pub(super) previous_position: [f32; 3],
    pub(super) current_position: [f32; 3],
    pub(super) displayed_position: [f32; 3],
    pub(super) previous_time_ms: u64,
    pub(super) current_time_ms: u64,
    pub(super) displayed_time_ms: u64,
}
struct Renderer {
    pub(super) surface: wgpu::Surface<'static>,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) config: wgpu::SurfaceConfiguration,
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) ui_pipeline: wgpu::RenderPipeline,
    pub(super) ui_bind_group: wgpu::BindGroup,
    pub(super) ui_vertex_buffer: wgpu::Buffer,
    pub(super) ui_vertex_count: u32,
    pub(super) console: Console,
    pub(super) vertex_buffer: wgpu::Buffer,
    pub(super) vertex_count: u32,
    pub(super) player_buffer: wgpu::Buffer,
    pub(super) remote_vertex_count: u32,
    pub(super) remote_vertices: Vec<Vertex>,
    pub(super) remote_players: [Option<RemotePlayerTrack>; MAX_VISIBLE_PLAYERS],
    pub(super) last_remote_snapshot_time_ms: Option<u64>,
    pub(super) last_remote_snapshot_received_at: Option<Instant>,
    pub(super) remote_geometry_dirty: bool,
    pub(super) camera_buffer: wgpu::Buffer,
    pub(super) camera_bind_group: wgpu::BindGroup,
    pub(super) depth_view: wgpu::TextureView,
    pub(super) size: PhysicalSize<u32>,
    pub(super) character: CharacterState,
    pub(super) health: u16,
    pub(super) collision_index: CollisionIndex,
    pub(super) collision_query: CollisionQuery,
    pub(super) yaw: f32,
    pub(super) pitch: f32,
    pub(super) keys: HashSet<KeyCode>,
    pub(super) last_cursor: Option<(f64, f64)>,
    pub(super) cursor_locked: bool,
    pub(super) last_frame: Instant,
    pub(super) fps_time: Instant,
    pub(super) fps_frames: u32,
    pub(super) frame_stats: crate::stats::FrameStats,
    pub(super) last_redraw: Instant,
    pub(super) fixed_gpu_bytes: u64,
    pub(super) previous_bytes_sent: u64,
    pub(super) previous_bytes_received: u64,
    pub(super) remote_player_count: usize,
    pub(super) map_fingerprint: u64,
    pub(super) profile_id: PlayerProfileId,
    pub(super) connector: Connector,
    pub(super) active_attempt: Option<AttemptId>,
    pub(super) session: Option<ClientSession>,
    pub(super) session_events: Vec<SessionEvent>,
    pub(super) local_server: Option<LocalServer>,
    pub(super) last_shot_sent_at: Option<Instant>,
    pub(super) hit_confirmation_until: Option<Instant>,
    pub(super) data_dir: PathBuf,
    pub(super) map_path: Option<PathBuf>,
    pub(super) threads: crate::options::ThreadLane,
    pub(super) physics_accumulator: f32,
    pub(super) jump_pending: bool,
    pub(super) input_keepalive: f32,
    pub(super) last_sent_direction: [f32; 2],
}

pub use window::run;
