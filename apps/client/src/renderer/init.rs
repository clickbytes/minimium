use super::*;

pub(super) struct RendererSetup {
    pub(super) window: Arc<Window>,
    pub(super) worlds: crate::bootstrap::InitialWorlds,
    pub(super) connection: crate::bootstrap::ClientConnection,
    pub(super) diagnostic_logger: minimium_diagnostics::DomainLogger,
    pub(super) renderer_logger: minimium_diagnostics::DomainLogger,
    pub(super) local_server_config: minimium_client_runtime::LocalServerLaunchConfig,
    pub(super) startup_settings: crate::application::StartupSettings,
    pub(super) config_file_settings: std::collections::BTreeMap<String, String>,
    pub(super) require_earth: bool,
    pub(super) demo: bool,
}

impl DesktopHost {
    pub(super) async fn new(setup: RendererSetup) -> Result<Self, Box<dyn Error>> {
        let RendererSetup {
            window,
            worlds,
            connection,
            diagnostic_logger,
            renderer_logger,
            local_server_config,
            startup_settings,
            config_file_settings,
            require_earth,
            demo,
        } = setup;
        let locale = startup_settings.locale();
        let sky_quality = startup_settings.sky_quality();
        let localizer = minimium_localization::Localizer::new(Some(diagnostic_logger.clone()))
            .map_err(std::io::Error::other)?;
        let crate::bootstrap::InitialWorlds {
            instance: current_instance,
            map,
            world_catalog,
            world_manifests,
        } = worlds;
        let current_manifest = world_manifests
            .get(&current_instance)
            .copied()
            .ok_or("initial world has no generation manifest")?;
        let mut sector_cache = sector_scene::ClientSectorCache::new()?;
        let mut bootstrap_terrain = None;
        let initial_scene = match crate::world_content::bootstrap_terrain_inputs(
            current_instance,
            current_manifest,
            &map,
        ) {
            Ok(terrain_inputs) => {
                bootstrap_terrain = Some(terrain_inputs.clone());
                sector_cache.ensure_scene(
                    sector_scene::SceneInterestOwner::Bootstrap(current_instance),
                    current_instance,
                    current_manifest,
                    map.spawn(),
                    minimium_renderer::terrain_view::TerrainView::new(map.spawn(), 1.287, 1_000),
                    [0.0; 3],
                    0,
                    &world_catalog,
                    &world_manifests,
                    None,
                    terrain_inputs,
                    None,
                )?
            }
            Err(error)
                if error.kind()
                    == minimium_procgen::GenerationErrorKind::Sampling(
                        minimium_procgen::EarthSampleError::PackageUnavailable,
                    ) =>
            {
                None
            }
            Err(error) => return Err(Box::new(error)),
        };
        let vertices: Vec<_> = initial_scene
            .as_ref()
            .into_iter()
            .flat_map(|scene| scene.cpu.sectors().iter())
            .flat_map(|(_, geometry)| sector_geometry(geometry))
            .collect();
        let dpi_scale = window.scale_factor();
        let mut gpu = Renderer::new(window, &vertices, sky_quality, &renderer_logger).await?;
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
        gpu.fixed_exposure_ev = startup_settings.exposure_ev();
        gpu.atmosphere_gpu.debug = startup_settings.atmosphere_debug() as u32;
        gpu.terrain_gpu.uniform.debug[0] = startup_settings.terrain_debug() as u32;
        // Pending collision jobs and bounded GPU uploads complete in the frame loop.
        let gpu_ready = initial_scene.as_ref().is_some_and(|scene| {
            gpu.cache_sector_scene(current_instance, scene)
                && gpu.activate_sector_scene(current_instance, scene, map.fingerprint())
        });
        gpu.scene_visible = !require_earth && gpu_ready;
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
        let connection = connection.for_host(crate::bootstrap::HostCapabilities::Desktop {
            compressed_materials: gpu
                .device
                .features()
                .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
                && gpu.device.limits().max_texture_dimension_2d >= 4096,
        });
        let mut setting_values = crate::application::SettingsStore::new(&startup_settings);
        setting_values.set_effective(
            crate::client_commands::ClientPropertyValue::SkyQuality(sky_quality_state.effective),
            sky_quality_state.fallback_reason.map(str::to_owned),
        );
        let input_mapper =
            crate::input::InputMapper::from_setting(&startup_settings.input_bindings())?;
        Ok(Self {
            cosmetic_epoch: Instant::now(),
            bolids: Vec::with_capacity(minimium_renderer::milky_way_gpu::MAX_BOLIDS),
            bolid_worker: bolids::BolidWorker::new()?,
            prepared_bolids: Default::default(),
            diagnostic_bolid_age: None,
            bolid_transmission_requested: None,
            bolid_transmission_revision: 0,
            last_sky_view_at: None,
            pending_bolid_request: None,
            localizer,
            renderer: gpu,
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
            camera_visual_offset: [0.0; 3],
            metrics: FrameMetrics {
                next_step_at: Instant::now() + Duration::from_millis(16),
                fps_time: Instant::now(),
                fps_frames: 0,
                frame_stats,
                last_redraw: Instant::now(),
            },
            demo_universe_enabled: startup_settings.sky_demo(),
            fov_degrees: startup_settings.fov(),
            mouse_sensitivity: startup_settings.mouse_sensitivity(),
            show_celestial_names: startup_settings.sky_names(),
            atmosphere_debug: startup_settings.atmosphere_debug(),
            moon_flux: startup_settings.moon_flux(),
            terrain_debug: startup_settings.terrain_debug(),
            sky_quality: sky_quality_state,
            environment_frame: EnvironmentFrameInput::unavailable(),
            render_environment_frame: EnvironmentFrame::unavailable(RenderViewFrame::default()),
            environment_observer: environment::observer::ObserverAdapter::default(),
            meteors: meteors::MeteorRuntime::configured(&startup_settings),
            impacts: Default::default(),
            sky_calculator: minimium_astronomy::SkyCalculator::default(),
            sky_catalogue,
            environment_error: None,
            weather: {
                let mut weather = clouds::WeatherState::new();
                weather.set_detail(startup_settings.cloud_detail());
                weather.debug = startup_settings.cloud_debug();
                weather
            },
            demo_environment: demo.then(LocalDemoEnvironment::new).flatten(),
            gameplay_presentation: GameplayPresentation {
                remote_vertices: Vec::with_capacity(16 * 36),
                remote_shadow_vertices: Vec::with_capacity(16 * 36),
                remote_geometry_dirty: false,
                hit_confirmation_until: None,
            },
            excavation_debug: Default::default(),
            held_tool_motion: Default::default(),
            application: crate::application::ClientApplication::new(
                crate::application::UiModel { console },
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
                current_instance,
                world_catalog,
                world_manifests,
                bootstrap_terrain,
                initial_scene.as_ref().map(|scene| scene.cpu.clone()),
                crate::application::WorldTransitionController::new(!require_earth && !gpu_ready),
                crate::application::MapOverviewModel::new(2.8),
                crate::application::ClientStatus::Status(if require_earth {
                    minimium_localization::UiStatus::EarthConnecting
                } else {
                    minimium_localization::UiStatus::EnvironmentLocalDemo
                }),
                crate::host_time::sample(),
            )?,
            pending_step_disconnect: None,
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
        self.renderer.resize(size);
        self.metrics.frame_stats.set_gpu_bytes(
            self.renderer.fixed_gpu_bytes + u64::from(size.width) * u64::from(size.height) * 4,
        );
    }
}
