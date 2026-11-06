use super::*;

pub(super) struct RendererSetup {
    pub(super) window: Arc<Window>,
    pub(super) map: WorldMap,
    pub(super) world_catalog:
        std::collections::HashMap<minimium_world::world::WorldInstanceId, WorldMap>,
    pub(super) world_manifests:
        std::collections::HashMap<minimium_world::world::WorldInstanceId, WorldManifest>,
    pub(super) profile_id: PlayerProfileId,
    pub(super) map_path: Option<PathBuf>,
    pub(super) world_definition_path: Option<PathBuf>,
    pub(super) local_server_config: minimium_client_runtime::LocalServerLaunchConfig,
    pub(super) sky_quality: minimium_renderer::quality::SkyQuality,
    pub(super) startup_fov: f32,
    pub(super) startup_mouse_sensitivity: f32,
    pub(super) startup_sky_names: bool,
    pub(super) startup_cloud_detail: u8,
    pub(super) startup_atmosphere_debug: minimium_renderer::atmosphere_debug::AtmosphereDebug,
    pub(super) startup_terrain_debug: minimium_renderer::terrain_debug::TerrainDebug,
    pub(super) startup_cloud_debug: minimium_environment::weather::CloudDebug,
    pub(super) startup_sky_demo: bool,
    pub(super) preference_sources:
        std::collections::BTreeMap<String, minimium_commands::PropertySource>,
    pub(super) config_file_settings: std::collections::BTreeMap<String, String>,
    pub(super) require_earth: bool,
    pub(super) demo: bool,
}

impl Renderer {
    pub(super) async fn new(setup: RendererSetup) -> Result<Self, Box<dyn Error>> {
        let RendererSetup {
            window,
            map,
            world_catalog,
            world_manifests,
            profile_id,
            map_path,
            world_definition_path,
            local_server_config,
            sky_quality,
            startup_fov,
            startup_mouse_sensitivity,
            startup_sky_names,
            startup_cloud_detail,
            startup_atmosphere_debug,
            startup_terrain_debug,
            startup_cloud_debug,
            startup_sky_demo,
            preference_sources,
            config_file_settings,
            require_earth,
            demo,
        } = setup;
        let map_fingerprint = map.fingerprint();
        let current_instance = world_catalog
            .iter()
            .find_map(|(instance, candidate)| {
                (candidate.fingerprint() == map_fingerprint).then_some(*instance)
            })
            .unwrap_or_else(|| {
                minimium_world::world::WorldInstanceId::new(1)
                    .expect("primary world identifier is nonzero")
            });
        let current_manifest = world_manifests
            .get(&current_instance)
            .copied()
            .ok_or("initial world has no generation manifest")?;
        let mut sector_cache = sector_scene::ClientSectorCache::new();
        let initial_scene = sector_cache.ensure_scene(
            current_instance,
            current_manifest,
            map.spawn(),
            minimium_renderer::terrain_view::TerrainView::legacy(map.spawn()),
            [0.0; 3],
            &world_catalog,
            &world_manifests,
            None,
        );
        let vertices: Vec<_> = initial_scene
            .as_ref()
            .into_iter()
            .flat_map(|scene| scene.sectors.iter())
            .flat_map(|(_, geometry)| sector_geometry(geometry))
            .collect();
        let mut gpu = GpuResources::new(window, &vertices, sky_quality).await?;
        let effective_quality = if gpu.shadow_gpu.settings.enabled {
            minimium_renderer::quality::SkyQuality::Standard
        } else {
            minimium_renderer::quality::SkyQuality::Low
        };
        let sky_quality_state = minimium_renderer::quality::SkyQualityState::new(
            sky_quality,
            effective_quality,
            gpu.shadow_gpu.fallback_reason,
        );
        gpu.atmosphere_gpu.debug = startup_atmosphere_debug as u32;
        gpu.terrain_gpu.uniform.debug[0] = startup_terrain_debug as u32;
        // Pending collision jobs and bounded GPU uploads complete in the frame loop.
        let gpu_ready = initial_scene.as_ref().is_some_and(|scene| {
            gpu.cache_sector_scene(current_instance, scene)
                && gpu.activate_sector_scene(current_instance, scene)
        });
        gpu.scene_visible = !require_earth && gpu_ready;
        let collision_index = initial_scene.as_ref().map_or_else(
            || minimium_simulation::CollisionIndex::from_geometry(Vec::new(), Vec::new()),
            |scene| scene.collision.clone(),
        );
        let sectors_ready = initial_scene.is_some();
        let active_sector_keys = initial_scene
            .as_ref()
            .map_or_else(Vec::new, |s| s.keys.clone());
        let mut frame_stats = crate::stats::FrameStats::default();
        frame_stats.set_gpu_bytes(
            gpu.fixed_gpu_bytes
                + u64::from(gpu.size.width.max(1)) * u64::from(gpu.size.height.max(1)) * 4,
        );
        let mut console = Console::new();
        console.push_line("F1 opens the command console. Type help for commands.");
        console.push_line("M toggles the world overview.");
        let sky_catalogue = match minimium_renderer::stars::StarCatalogueAdapter::load_embedded() {
            Ok(catalogue) => Some(catalogue),
            Err(error) => {
                console.push_error(format!("Embedded star catalog unavailable: {error}"));
                None
            }
        };
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
        connection.set_presentation(
            gpu.device
                .features()
                .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
                && gpu.device.limits().max_texture_dimension_2d >= 4096,
        );
        connection.set_world_fingerprints(fingerprints);
        Ok(Self {
            gpu,
            game: client_game::ClientGameState {
                gameplay: GameplayView {
                    remote_vertices: Vec::with_capacity(16 * 36),
                    remote_shadow_vertices: Vec::with_capacity(16 * 36),
                    reconciliation: minimium_client_runtime::SnapshotReconciler::default(),
                    remote_positions_mm: Vec::with_capacity(MAX_VISIBLE_PLAYERS),
                    remote_geometry_dirty: false,
                    prediction: minimium_client_runtime::ClientPrediction::new(
                        CharacterState::at_spawn_with_gravity(map.spawn(), gravity),
                        None,
                    ),
                    health: PLAYER_MAX_HEALTH,
                    collision_index,
                    collision_query: CollisionQuery::default(),
                    last_shot_sent_at: None,
                    hit_confirmation_until: None,
                    sectors_ready,
                },
                world_manifests,
                current_instance,
                session_context: None,
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
                local_server_config,
                require_earth,
                demo,
                config_file_settings,
                preference_sources,
            },
            demo_universe_enabled: startup_sky_demo,
            fov_base: startup_fov,
            fov_degrees: startup_fov,
            fov_override: None,
            mouse_sensitivity_base: startup_mouse_sensitivity,
            mouse_sensitivity: startup_mouse_sensitivity,
            mouse_sensitivity_override: None,
            sky_names_base: startup_sky_names,
            sky_names_override: None,
            cloud_detail_base: startup_cloud_detail,
            cloud_detail_override: None,
            show_celestial_names: startup_sky_names,
            sky_quality_base: sky_quality,
            sky_quality_override: None,
            atmosphere_debug: startup_atmosphere_debug,
            atmosphere_debug_base: startup_atmosphere_debug,
            atmosphere_debug_override: None,
            terrain_debug: startup_terrain_debug,
            terrain_debug_base: startup_terrain_debug,
            terrain_debug_override: None,
            cloud_debug_base: startup_cloud_debug,
            cloud_debug_override: None,
            sky_demo_base: startup_sky_demo,
            sky_demo_override: None,
            sky_quality: sky_quality_state,
            connection,
            environment_frame: EnvironmentFrameInput::unavailable(),
            render_environment_frame: EnvironmentFrame::unavailable(RenderViewFrame::default()),
            environment_observer: environment::observer::ObserverAdapter::default(),
            sky_calculator: minimium_astronomy::SkyCalculator::default(),
            sky_catalogue,
            environment_error: None,
            weather: {
                let mut weather = clouds::WeatherState::new();
                weather.set_detail(startup_cloud_detail);
                weather.debug = startup_cloud_debug;
                weather
            },
            demo_environment: demo.then(LocalDemoEnvironment::new).flatten(),
            world_catalog,
            sector_cache,
            active_sector_keys,
            last_render_position: if gpu_ready {
                map.spawn()
            } else {
                [f32::INFINITY; 3]
            },
            pending_sector_transitions: std::collections::HashMap::new(),
            overview_open: false,
            overview_instance: None,
            connection_status: if require_earth {
                "Łączenie z Earth".into()
            } else {
                "Demo".into()
            },
            overview_yaw: 0.0,
            overview_pitch: 0.0,
            overview_zoom: 2.8,
            overview_dragging: false,
            overview_slider_dragging: false,
            overview_dragged: false,
            overview_drag_distance: 0.0,
            map_teleport_armed: false,
            map_teleport_pending: false,
            teleport_loading: !require_earth && !gpu_ready,
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
