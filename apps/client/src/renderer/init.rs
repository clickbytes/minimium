use super::*;

pub(super) struct RendererSetup {
    pub(super) window: Arc<Window>,
    pub(super) map: WorldMap,
    pub(super) world_catalog:
        std::collections::HashMap<minimium_core::world::WorldInstanceId, WorldMap>,
    pub(super) world_manifests:
        std::collections::HashMap<minimium_core::world::WorldInstanceId, WorldManifest>,
    pub(super) profile_id: PlayerProfileId,
    pub(super) data_dir: PathBuf,
    pub(super) map_path: Option<PathBuf>,
    pub(super) world_definition_path: Option<PathBuf>,
    pub(super) threads: crate::options::ThreadLane,
    pub(super) require_earth: bool,
}

impl Renderer {
    pub(super) async fn new(setup: RendererSetup) -> Result<Self, Box<dyn Error>> {
        let RendererSetup {
            window,
            map,
            world_catalog,
            world_manifests,
            profile_id,
            data_dir,
            map_path,
            world_definition_path,
            threads,
            require_earth,
        } = setup;
        let map_fingerprint = map.fingerprint();
        let current_instance = world_catalog
            .iter()
            .find_map(|(instance, candidate)| {
                (candidate.fingerprint() == map_fingerprint).then_some(*instance)
            })
            .unwrap_or_else(|| {
                minimium_core::world::WorldInstanceId::new(1)
                    .expect("primary world identifier is nonzero")
            });
        let current_manifest = world_manifests
            .get(&current_instance)
            .copied()
            .ok_or("initial world has no generation manifest")?;
        let mut sector_cache = sector_scene::ClientSectorCache::new();
        let initial_scene = sector_cache
            .ensure_scene(
                current_instance,
                current_manifest,
                map.spawn(),
                &world_catalog,
                &world_manifests,
            )
            .ok_or("initial world sectors could not be installed")?;
        let vertices: Vec<_> = initial_scene
            .sectors
            .iter()
            .flat_map(|(_, geometry)| sector_geometry(geometry))
            .collect();
        let mut gpu = GpuResources::new(window, &vertices).await?;
        gpu.scene_visible = !require_earth;
        if !gpu.cache_sector_scene(current_instance, &initial_scene)
            || !gpu.activate_sector_scene(current_instance, &initial_scene)
        {
            return Err("initial world GPU sectors could not be installed".into());
        }
        let collision_index = initial_scene.collision.clone();
        let mut frame_stats = crate::stats::FrameStats::default();
        frame_stats.set_gpu_bytes(
            gpu.fixed_gpu_bytes
                + u64::from(gpu.size.width.max(1)) * u64::from(gpu.size.height.max(1)) * 4,
        );
        let mut console = Console::new();
        console.push_line("F1 opens the command console. Type help for commands.");
        console.push_line("M toggles the world overview.");
        let fingerprints = world_catalog
            .iter()
            .map(|(instance, map)| (*instance, map.fingerprint()))
            .collect();
        let gravity = if map_path.is_none() && world_definition_path.is_none() {
            CharacterGravity::Radial { center: [0.0; 3] }
        } else {
            CharacterGravity::FixedY
        };
        let mut connection = ConnectionController::new(profile_id, map_fingerprint);
        connection.set_world_fingerprints(fingerprints);
        Ok(Self {
            gpu,
            game: GameplayView {
                remote_vertices: Vec::with_capacity(16 * 36),
                remote_players: [None; MAX_VISIBLE_PLAYERS],
                last_remote_snapshot_time_ms: None,
                last_remote_snapshot_received_at: None,
                remote_geometry_dirty: false,
                character: CharacterState::at_spawn_with_gravity(map.spawn(), gravity),
                position_mm: None,
                health: PLAYER_MAX_HEALTH,
                collision_index,
                collision_query: CollisionQuery::default(),
                remote_player_count: 0,
                last_shot_sent_at: None,
                hit_confirmation_until: None,
                physics_accumulator: 0.0,
                sectors_ready: true,
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
                world_definition_path,
                threads,
                require_earth,
            },
            connection,
            world_catalog,
            world_manifests,
            sector_cache,
            active_sector_keys: initial_scene.keys,
            last_render_position: map.spawn(),
            pending_sector_transitions: std::collections::HashMap::new(),
            current_instance,
            overview_open: false,
            overview_instance: None,
            connection_status: if require_earth {
                "Łączenie z Earth".into()
            } else {
                "Demo".into()
            },
            globe_loading: None,
            globe_detail_loading: None,
            overview_yaw: 0.0,
            overview_pitch: 0.0,
            overview_zoom: 2.8,
            overview_dragging: false,
            overview_slider_dragging: false,
            overview_dragged: false,
            overview_drag_distance: 0.0,
            map_teleport_armed: false,
            map_teleport_pending: false,
            teleport_loading: false,
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
