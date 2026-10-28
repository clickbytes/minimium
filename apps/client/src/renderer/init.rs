use super::*;

impl Renderer {
    pub(super) async fn new(
        window: Arc<Window>,
        map: WorldMap,
        profile_id: PlayerProfileId,
        data_dir: PathBuf,
        map_path: Option<PathBuf>,
        threads: crate::options::ThreadLane,
    ) -> Result<Self, Box<dyn Error>> {
        let vertices = map_geometry(&map);
        let map_fingerprint = map.fingerprint();
        let collision_index = map.collision_index();
        let gpu = GpuResources::new(window, &vertices).await?;
        let mut frame_stats = crate::stats::FrameStats::default();
        frame_stats.set_gpu_bytes(
            gpu.fixed_gpu_bytes
                + u64::from(gpu.size.width.max(1)) * u64::from(gpu.size.height.max(1)) * 4,
        );
        let mut console = Console::new();
        console.push_line("F1 opens the command console. Type help for commands.");
        Ok(Self {
            gpu,
            game: GameplayView {
                remote_vertices: Vec::with_capacity(16 * 36),
                remote_players: [None; MAX_VISIBLE_PLAYERS],
                last_remote_snapshot_time_ms: None,
                last_remote_snapshot_received_at: None,
                remote_geometry_dirty: false,
                character: CharacterState::at_spawn(map.spawn()),
                health: PLAYER_MAX_HEALTH,
                collision_index,
                collision_query: CollisionQuery::default(),
                planet: map.planet(),
                portals: map.portals().to_vec(),
                remote_player_count: 0,
                last_shot_sent_at: None,
                hit_confirmation_until: None,
                physics_accumulator: 0.0,
            },
            input: InputState {
                yaw: -std::f32::consts::FRAC_PI_2,
                pitch: 0.0,
                keys: HashSet::new(),
                last_cursor: None,
                cursor_locked: false,
                jump_pending: false,
                input_keepalive: 0.0,
                last_sent_direction: [0.0; 2],
            },
            ui: UiState {
                console,
                session_events: Vec::with_capacity(16),
            },
            metrics: FrameMetrics {
                last_frame: Instant::now(),
                fps_time: Instant::now(),
                fps_frames: 0,
                frame_stats,
                last_redraw: Instant::now(),
                previous_bytes_sent: 0,
                previous_bytes_received: 0,
            },
            settings: ClientSettings {
                data_dir,
                map_path,
                threads,
            },
            connection: ConnectionController::new(profile_id, map_fingerprint),
        })
    }

    pub(super) fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.gpu.resize(size);
        self.metrics.frame_stats.set_gpu_bytes(
            self.gpu.fixed_gpu_bytes + u64::from(size.width) * u64::from(size.height) * 4,
        );
    }
}
