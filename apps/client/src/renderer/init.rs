use super::*;
use minimium_client_runtime::ConnectionController;

pub(super) struct RendererSetup {
    pub(super) window: Arc<Window>,
    pub(super) map: WorldMap,
    pub(super) world_catalog:
        std::collections::HashMap<minimium_world::world::WorldInstanceId, WorldMap>,
    pub(super) world_manifests:
        std::collections::HashMap<minimium_world::world::WorldInstanceId, WorldManifest>,
    pub(super) profile_id: PlayerProfileId,
    pub(super) diagnostic_logger: minimium_diagnostics::DomainLogger,
    pub(super) renderer_logger: minimium_diagnostics::DomainLogger,
    pub(super) content_cache_dir: PathBuf,
    pub(super) content_cache_limit_bytes: u64,
    pub(super) earth_package_dir: PathBuf,
    pub(super) map_path: Option<PathBuf>,
    pub(super) world_definition_path: Option<PathBuf>,
    pub(super) local_server_config: minimium_client_runtime::LocalServerLaunchConfig,
    pub(super) sky_quality: minimium_renderer::quality::SkyQuality,
    pub(super) startup_fov: f32,
    pub(super) startup_mouse_sensitivity: f32,
    pub(super) startup_input_bindings: String,
    pub(super) startup_sky_names: bool,
    pub(super) startup_cloud_detail: u8,
    pub(super) startup_atmosphere_debug: minimium_renderer::atmosphere_debug::AtmosphereDebug,
    pub(super) startup_terrain_debug: minimium_renderer::terrain_debug::TerrainDebug,
    pub(super) startup_cloud_debug: minimium_environment::weather::CloudDebug,
    pub(super) startup_sky_demo: bool,
    pub(super) meteor_preferences: std::collections::BTreeMap<String, String>,
    pub(super) preference_sources:
        std::collections::BTreeMap<String, minimium_commands::PropertySource>,
    pub(super) config_file_settings: std::collections::BTreeMap<String, String>,
    pub(super) require_earth: bool,
    pub(super) demo: bool,
    pub(super) locale: minimium_localization::Locale,
}

impl Renderer {
    pub(super) async fn new(setup: RendererSetup) -> Result<Self, Box<dyn Error>> {
        let RendererSetup {
            window,
            map,
            world_catalog,
            world_manifests,
            profile_id,
            diagnostic_logger,
            renderer_logger,
            content_cache_dir,
            content_cache_limit_bytes,
            earth_package_dir,
            map_path,
            world_definition_path,
            local_server_config,
            sky_quality,
            startup_fov,
            startup_mouse_sensitivity,
            startup_input_bindings,
            startup_sky_names,
            startup_cloud_detail,
            startup_atmosphere_debug,
            startup_terrain_debug,
            startup_cloud_debug,
            startup_sky_demo,
            meteor_preferences,
            preference_sources,
            config_file_settings,
            require_earth,
            demo,
            locale,
        } = setup;
        let localizer = minimium_localization::Localizer::new(Some(diagnostic_logger.clone()))
            .map_err(std::io::Error::other)?;
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
        let mut sector_cache = sector_scene::ClientSectorCache::new()?;
        let initial_scene = sector_cache.ensure_scene(
            current_instance,
            current_manifest,
            map.spawn(),
            minimium_renderer::terrain_view::TerrainView::new(map.spawn(), 1.287, 1_000),
            [0.0; 3],
            0,
            &world_catalog,
            &world_manifests,
            None,
        )?;
        let vertices: Vec<_> = initial_scene
            .as_ref()
            .into_iter()
            .flat_map(|scene| scene.sectors.iter())
            .flat_map(|(_, geometry)| sector_geometry(geometry))
            .collect();
        let dpi_scale = window.scale_factor();
        let mut gpu = GpuResources::new(window, &vertices, sky_quality, &renderer_logger).await?;
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
                && gpu.activate_sector_scene(current_instance, scene, map.fingerprint())
        });
        gpu.scene_visible = !require_earth && gpu_ready;
        let collision_index = initial_scene.as_ref().map_or_else(
            || minimium_simulation::CollisionIndex::from_geometry(Vec::new(), Vec::new()),
            |scene| scene.collision.clone(),
        );
        let sectors_ready = initial_scene.is_some();
        let mut frame_stats = crate::stats::FrameStats::default();
        frame_stats.set_gpu_bytes(
            gpu.fixed_gpu_bytes
                + u64::from(gpu.size.width.max(1)) * u64::from(gpu.size.height.max(1)) * 4,
        );
        let mut console = Console::new();
        console.set_dpi_scale(dpi_scale);
        console.push_ui_message(
            minimium_localization::UiMessage::Notice(
                minimium_localization::UiNotice::ConsoleWelcome,
            ),
            &localizer,
            locale,
        );
        console.push_ui_message(
            minimium_localization::UiMessage::Notice(
                minimium_localization::UiNotice::ConsoleOverview,
            ),
            &localizer,
            locale,
        );
        let sky_catalogue = match minimium_renderer::stars::StarCatalogueAdapter::load_embedded() {
            Ok(catalogue) => Some(catalogue),
            Err(error) => {
                if let Ok(code) =
                    minimium_diagnostics::EventCode::new("renderer.star_catalog_unavailable")
                {
                    diagnostic_logger.emit(
                        diagnostic_logger
                            .event(minimium_diagnostics::Level::Error, code)
                            .field("error", &error.to_string()),
                    );
                }
                console.push_ui_error_message(
                    minimium_localization::UiMessage::Notice(
                        minimium_localization::UiNotice::StarCatalogUnavailable,
                    ),
                    &localizer,
                    locale,
                );
                None
            }
        };
        let gravity = if map_path.is_none() && world_definition_path.is_none() {
            CharacterGravity::Radial { center: [0.0; 3] }
        } else {
            CharacterGravity::FixedY
        };
        let mut connection = ConnectionController::new();
        connection.register_replica_schema(
            minimium_game_sample::PORTAL_COMPONENT,
            minimium_game_sample::PORTAL_SCHEMA,
        )?;
        connection.set_authentication(minimium_client_runtime::ClientAuthentication::new(
            "dev.profile",
            profile_id.as_bytes().to_vec(),
        )?);
        connection.set_diagnostic_logger(diagnostic_logger);
        connection.set_content_cache_dir(content_cache_dir);
        connection.set_content_cache_limit(content_cache_limit_bytes);
        connection.set_world_content_resolver(Arc::new(
            crate::world_content::SampleWorldContentResolver::new(
                world_catalog.clone(),
                world_manifests.clone(),
                Some(earth_package_dir),
            ),
        ));
        connection.set_presentation(
            gpu.device
                .features()
                .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
                && gpu.device.limits().max_texture_dimension_2d >= 4096,
        );
        let mut setting_values = crate::application::SettingsStore::new(
            [
                (
                    "ui.locale".to_owned(),
                    match locale {
                        minimium_localization::Locale::English => "en".to_owned(),
                        minimium_localization::Locale::Polish => "pl".to_owned(),
                    },
                ),
                ("client.camera.fov".to_owned(), startup_fov.to_string()),
                (
                    "client.input.mouse_sensitivity".to_owned(),
                    startup_mouse_sensitivity.to_string(),
                ),
                (
                    "client.input.bindings".to_owned(),
                    startup_input_bindings.clone(),
                ),
                (
                    "client.graphics.sky_quality".to_owned(),
                    sky_quality.label().to_owned(),
                ),
                ("client.sky.names".to_owned(), startup_sky_names.to_string()),
                (
                    "client.weather.cloud_detail".to_owned(),
                    startup_cloud_detail.to_string(),
                ),
                (
                    "client.debug.atmosphere".to_owned(),
                    format!("{startup_atmosphere_debug:?}").to_ascii_lowercase(),
                ),
                (
                    "client.debug.terrain".to_owned(),
                    format!("{startup_terrain_debug:?}").to_ascii_lowercase(),
                ),
                (
                    "client.debug.clouds".to_owned(),
                    format!("{startup_cloud_debug:?}").to_ascii_lowercase(),
                ),
                ("client.sky.demo".to_owned(), startup_sky_demo.to_string()),
            ]
            .into_iter()
            .chain(meteor_preferences.clone())
            .collect(),
            preference_sources,
        );
        setting_values.set_effective(
            "client.graphics.sky_quality",
            sky_quality_state.effective.label().to_owned(),
            sky_quality_state.fallback_reason.map(str::to_owned),
        );
        let input_mapper = crate::input::InputMapper::from_setting(&startup_input_bindings)?;
        Ok(Self {
            bolids: Vec::with_capacity(minimium_renderer::milky_way_gpu::MAX_BOLIDS),
            prepared_bolids: Default::default(),
            diagnostic_bolid_age: None,
            last_sky_view_at: None,
            pending_bolid_request: None,
            localizer,
            gpu,
            shadow_state: shadows::ShadowState::default(),
            prepared_shadow: shadow_prepare::PreparedShadowFrame::invalid(
                shadows::ShadowInvalidReason::InvalidFrame,
            ),
            shadow_map_preparation: shadow_map_cache::ShadowMapPreparationCache::default(),
            input: InputState {
                yaw: -std::f32::consts::FRAC_PI_2,
                pitch: 0.0,
                last_cursor: None,
                cursor_locked: false,
                jump_pending: false,
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
            demo_universe_enabled: startup_sky_demo,
            fov_degrees: startup_fov,
            mouse_sensitivity: startup_mouse_sensitivity,
            show_celestial_names: startup_sky_names,
            atmosphere_debug: startup_atmosphere_debug,
            terrain_debug: startup_terrain_debug,
            sky_quality: sky_quality_state,
            environment_frame: EnvironmentFrameInput::unavailable(),
            render_environment_frame: EnvironmentFrame::unavailable(RenderViewFrame::default()),
            environment_observer: environment::observer::ObserverAdapter::default(),
            meteors: meteors::MeteorRuntime::configured(&meteor_preferences),
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
            gameplay_presentation: GameplayPresentation {
                remote_vertices: Vec::with_capacity(16 * 36),
                remote_shadow_vertices: Vec::with_capacity(16 * 36),
                remote_geometry_dirty: false,
                last_shot_sent_at: None,
                hit_confirmation_until: None,
            },
            application: crate::application::ClientApplication::new(
                crate::application::UiModel {
                    console,
                    session_events: Vec::with_capacity(16),
                },
                crate::application::ClientGameState::new(
                    crate::application::GameplayState {
                        reconciliation: minimium_client_runtime::SnapshotReconciler::default(),
                        remote_positions_mm: Vec::with_capacity(MAX_VISIBLE_PLAYERS),
                        prediction: minimium_client_runtime::ClientPrediction::new(
                            CharacterState::at_spawn_with_gravity(map.spawn(), gravity),
                            None,
                        ),
                        health: PLAYER_MAX_HEALTH,
                        collision_index,
                        collision_query: CollisionQuery::default(),
                        collision_revision: 1,
                        sectors_ready,
                    },
                    current_instance,
                    world_manifests,
                ),
                connection,
                crate::application::ClientSettings::new(
                    locale,
                    local_server_config,
                    require_earth,
                    demo,
                    config_file_settings,
                    setting_values,
                ),
                input_mapper,
                world_catalog,
                crate::application::WorldTransitionController::new(!require_earth && !gpu_ready),
                crate::application::MapOverviewModel::new(2.8),
                crate::application::ClientStatus::Status(if require_earth {
                    minimium_localization::UiStatus::EarthConnecting
                } else {
                    minimium_localization::UiStatus::EnvironmentLocalDemo
                }),
            ),
            sector_cache,
            last_render_position: if gpu_ready {
                map.spawn()
            } else {
                [f32::INFINITY; 3]
            },
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
