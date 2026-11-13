use super::*;
use crate::messages::ClientText as Text;

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    world_map: Option<WorldMap>,
    world_catalog:
        Option<std::collections::HashMap<minimium_world::world::WorldInstanceId, WorldMap>>,
    world_manifests: Option<
        std::collections::HashMap<
            minimium_world::world::WorldInstanceId,
            minimium_world::world::WorldManifest,
        >,
    >,
    profile_id: PlayerProfileId,
    options: crate::options::ClientOptions,
    data_dir: PathBuf,
    cache_dir: PathBuf,
    log_dir: PathBuf,
    earth_package_dir: PathBuf,
    parent_run_id: String,
    diagnostic_logger: minimium_diagnostics::DomainLogger,
    renderer_logger: minimium_diagnostics::DomainLogger,
    localizer: minimium_localization::Localizer,
    _diagnostics: minimium_telemetry::logging::DiagnosticsRuntime,
    startup_error: Option<String>,
    modifiers: winit::keyboard::ModifiersState,
    dev_input: Option<std::sync::mpsc::Receiver<String>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = super::window_mode::attributes(event_loop, self.options.windowed)
            .with_title(Text::WindowTitle.localized(&self.localizer, self.options.locale));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.startup_error = Some(
                    Text::WindowCreationFailed(error.to_string())
                        .localized(&self.localizer, self.options.locale),
                );
                event_loop.exit();
                return;
            }
        };
        if self.options.windowed {
            super::window_mode::fit_windowed(&window, event_loop);
        }
        let world_map = self.world_map.take().unwrap_or_else(|| {
            panic!(
                "{}",
                Text::RendererAlreadyInitialized.localized(&self.localizer, self.options.locale)
            )
        });
        match pollster::block_on(Renderer::new(init::RendererSetup {
            window: Arc::clone(&window),
            map: world_map,
            world_catalog: self
                .world_catalog
                .take()
                .expect("world catalog is available before renderer startup"),
            world_manifests: self
                .world_manifests
                .take()
                .expect("world manifests are available before renderer startup"),
            profile_id: self.profile_id,
            diagnostic_logger: self.diagnostic_logger.clone(),
            renderer_logger: self.renderer_logger.clone(),
            content_cache_dir: self.cache_dir.clone(),
            content_cache_limit_bytes: self.options.cache_limit_bytes,
            earth_package_dir: self.earth_package_dir.clone(),
            map_path: self.options.map_path.clone(),
            world_definition_path: self.options.world_definition_path.clone(),
            local_server_config: self
                .options
                .local_server_launch_config(
                    self.data_dir.clone(),
                    self.log_dir.clone(),
                    self.parent_run_id.clone(),
                    None,
                )
                .expect("validated local server launch options"),
            sky_quality: self.options.sky_quality,
            startup_fov: self.options.startup_fov,
            startup_mouse_sensitivity: self.options.startup_mouse_sensitivity,
            startup_input_bindings: self.options.startup_input_bindings.clone(),
            startup_sky_names: self.options.startup_sky_names,
            startup_cloud_detail: self.options.startup_cloud_detail,
            startup_atmosphere_debug: self.options.startup_atmosphere_debug,
            startup_terrain_debug: self.options.startup_terrain_debug,
            startup_cloud_debug: self.options.startup_cloud_debug,
            startup_sky_demo: self.options.startup_sky_demo,
            preference_sources: self.options.preference_sources.clone(),
            config_file_settings: self.options.config_file_settings.clone(),
            require_earth: self.options.require_earth,
            demo: self.options.demo,
            locale: self.options.locale,
        })) {
            Ok(mut renderer) => {
                if !self.options.demo {
                    if let Some(address) = &self.options.connect {
                        renderer.start_remote_attempt(address);
                    } else {
                        renderer.start_local_attempt(None);
                    }
                }
                self.window = Some(window);
                self.renderer = Some(renderer);
            }
            Err(error) => {
                self.renderer_logger.emit(
                    self.renderer_logger
                        .event(
                            minimium_diagnostics::Level::Error,
                            minimium_diagnostics::EventCode::new("renderer.initialize_failed")
                                .expect("static renderer event code is valid"),
                        )
                        .field("reason", &error.to_string()),
                );
                self.startup_error = Some(
                    Text::GraphicsInitializationFailed(error.to_string())
                        .localized(&self.localizer, self.options.locale),
                );
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let (Some(window), Some(renderer)) = (&self.window, &mut self.renderer) else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => renderer.resize(size),
            WindowEvent::ScaleFactorChanged { .. } => {
                renderer
                    .application
                    .ui
                    .console
                    .set_dpi_scale(window.scale_factor());
            }
            WindowEvent::Focused(false) => {
                renderer.meteors.audio.clear();
                renderer.application.input.clear_held();
                renderer.input.jump_pending = false;
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                super::window_events::keyboard(renderer, window, event_loop, event, self.modifiers)
            }
            WindowEvent::Ime(ime) => match ime {
                winit::event::Ime::Preedit(text, _) if renderer.application.ui.console.open => {
                    renderer.application.ui.console.set_preedit(&text);
                }
                winit::event::Ime::Commit(text) if renderer.application.ui.console.open => {
                    renderer.application.ui.console.commit_preedit(
                        &text,
                        &renderer.localizer,
                        renderer.application.settings.locale,
                    );
                }
                winit::event::Ime::Disabled => renderer.application.ui.console.clear_preedit(),
                _ => {}
            },
            WindowEvent::MouseInput { state, button, .. } => {
                super::window_events::mouse_input(renderer, window, state, button)
            }
            WindowEvent::MouseWheel { delta, .. } => {
                super::window_events::mouse_wheel(renderer, delta)
            }
            WindowEvent::CursorMoved { position, .. } => {
                super::window_events::cursor_moved(renderer, window, position)
            }
            WindowEvent::RedrawRequested => {
                let started = Instant::now();
                let interval = started.saturating_duration_since(renderer.metrics.last_redraw);
                renderer.metrics.last_redraw = started;
                let mut prepared_environment = renderer.update();
                let client_view = renderer.application.game.view();
                prepared_environment = prepared_environment.with_client_view(&client_view);
                let scene = super::scene_bridge::RenderSceneSnapshot::new(
                    renderer.prepared_bolids,
                    &prepared_environment,
                    &renderer.prepared_shadow,
                    renderer.gpu.world_geometry.publication().cloned(),
                );
                let draws = match renderer.gpu.render(&scene) {
                    Ok(draws) => draws,
                    Err(
                        wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated,
                    ) => {
                        renderer.resize(window.inner_size());
                        0
                    }
                    Err(error) => {
                        self.renderer_logger.emit(
                            self.renderer_logger
                                .event(
                                    minimium_diagnostics::Level::Error,
                                    minimium_diagnostics::EventCode::new("renderer.frame_failed")
                                        .expect("static renderer event code is valid"),
                                )
                                .field("reason", &format!("{error:?}")),
                        );
                        0
                    }
                };
                renderer
                    .application
                    .ui
                    .console
                    .restore_vertices(prepared_environment.take_ui_vertices());
                renderer.update_gpu_memory_metric();
                renderer.record_environment_render_metrics(draws > 0);
                renderer
                    .metrics
                    .frame_stats
                    .record_frame(interval, started.elapsed(), draws);
                if draws > 0 {
                    renderer.metrics.fps_frames += 1;
                }
                let sample_time = renderer.metrics.fps_time.elapsed().as_secs_f64();
                if sample_time >= 1.0 {
                    let fps = renderer.metrics.fps_frames as f64 / sample_time;
                    renderer.metrics.fps_frames = 0;
                    renderer.metrics.fps_time = Instant::now();
                    let coordinates = if renderer.application.settings.require_earth
                        && renderer.gpu.scene_visible
                    {
                        let [x, y, z] = renderer
                            .application
                            .game
                            .gameplay
                            .prediction
                            .character
                            .position
                            .map(f64::from);
                        let radius = (x * x + y * y + z * z).sqrt();
                        format!(
                            " | {:.4}°N {:.4}°E",
                            (y / radius).asin().to_degrees(),
                            z.atan2(x).to_degrees()
                        )
                    } else {
                        String::new()
                    };
                    window.set_title(&format!(
                        "{} | {}{} | {}",
                        Text::WindowTitleStatus {
                            fps,
                            health: renderer.application.game.gameplay.health
                        }
                        .localized(&renderer.localizer, renderer.application.settings.locale),
                        renderer
                            .application
                            .status
                            .format(&renderer.localizer, renderer.application.settings.locale),
                        coordinates,
                        crate::environment::localized_title_suffix(
                            renderer.environment_frame,
                            &renderer.localizer,
                            renderer.application.settings.locale,
                        )
                    ));
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if let Some(renderer) = &mut self.renderer {
            super::window_events::device_event(renderer, event);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let (Some(renderer), Some(input)) = (&mut self.renderer, &self.dev_input) {
            if renderer.gpu.scene_visible && !renderer.application.map_teleport_pending() {
                if let Ok(line) = input.try_recv() {
                    if super::dev_input::execute(renderer, &line) {
                        event_loop.exit();
                    }
                }
            }
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }
}

pub fn run(options: crate::options::ClientOptions) -> Result<(), Box<dyn Error>> {
    let initial_log_dir = options.resolved_log_dir()?;
    let diagnostics = minimium_telemetry::logging::DiagnosticsRuntime::start(&initial_log_dir)?;
    let diagnostic_logger = diagnostics.logger(minimium_diagnostics::DomainId::ClientRuntime);
    let localizer = minimium_localization::Localizer::new(Some(diagnostic_logger.clone()))
        .map_err(std::io::Error::other)?;
    let asset_io_logger = diagnostics.logger(minimium_diagnostics::DomainId::AssetIo);
    let procgen_logger = diagnostics.logger(minimium_diagnostics::DomainId::Procgen);
    let map_path = options.map_path.clone();
    let world_definition_path = options.world_definition_path.clone();
    let (map, world_catalog, world_manifests) = if let Some(path) = map_path.as_deref() {
        let map = minimium_asset_io::load_map(path).map_err(|error| {
            crate::diagnostics::failure(&asset_io_logger, error.code(), &error.to_string());
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .localized(&localizer, options.locale)
        })?;
        let mut catalog = std::collections::HashMap::new();
        let instance = minimium_world::world::WorldInstanceId::new(1)
            .expect("authored world instance is nonzero");
        catalog.insert(instance, map.clone());
        let manifests = std::collections::HashMap::from([(
            instance,
            crate::world_content::authored_manifest(1, &map),
        )]);
        (map, catalog, manifests)
    } else if let Some(path) = world_definition_path.as_deref() {
        let definition = minimium_asset_io::load_world_definition(path).map_err(|error| {
            crate::diagnostics::failure(&asset_io_logger, error.code(), &error.to_string());
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .localized(&localizer, options.locale)
        })?;
        minimium_procgen::validate_world_manifest(definition.manifest()).map_err(|error| {
            crate::diagnostics::failure(&procgen_logger, error.code(), &error.to_string());
            Text::BundledMapInvalid(error.to_string()).localized(&localizer, options.locale)
        })?;
        let map =
            minimium_procgen::materialize_dungeon_map(definition.dungeon()).map_err(|error| {
                crate::diagnostics::failure(&procgen_logger, error.code(), &error.to_string());
                Text::BundledMapInvalid(error.to_string()).localized(&localizer, options.locale)
            })?;
        let instance = minimium_world::world::WorldInstanceId::new(4)
            .expect("manual world instance ID is nonzero");
        let catalog = std::collections::HashMap::from([(instance, map.clone())]);
        let manifests = std::collections::HashMap::from([(instance, definition.manifest())]);
        (map, catalog, manifests)
    } else {
        let worlds = minimium_game_sample::generate_demo_worlds().map_err(|error| {
            procgen_logger.emit(
                procgen_logger
                    .event(
                        minimium_diagnostics::Level::Error,
                        minimium_diagnostics::EventCode::new(error.code())
                            .expect("static procgen event code is valid"),
                    )
                    .field("reason", &error.to_string()),
            );
            Text::BundledMapInvalid(error.to_string()).localized(&localizer, options.locale)
        })?;
        let mut catalog = std::collections::HashMap::new();
        let mut manifests = std::collections::HashMap::new();
        for world in worlds {
            manifests.insert(world.instance, world.manifest);
            catalog.insert(world.instance, world.map);
        }
        let map = catalog
            .get(&minimium_world::world::WorldInstanceId::new(1).expect("planet ID is nonzero"))
            .expect("demo generator always returns a planet")
            .clone();
        (map, catalog, manifests)
    };
    let paths = options.paths(map.fingerprint())?;
    let profile_id = minimium_platform::identity_store::load_or_create(&paths.profile_dir)?;
    let data_dir = paths.data_dir.clone();
    let cache_dir = paths.cache_dir.clone();
    let log_dir = paths.log_dir.clone();
    let logger = diagnostic_logger;
    logger.emit(logger.event(
        minimium_diagnostics::Level::Info,
        minimium_diagnostics::EventCode::new("client.starting")?,
    ));
    let parent_run_id = diagnostics.run_id().to_owned();
    let diagnostic_logger = logger.clone();
    let dev_input = if options.dev_stdin {
        let (sender, receiver) = std::sync::mpsc::sync_channel(32);
        crate::spawn_console_input_reader(sender, diagnostic_logger.clone());
        Some(receiver)
    } else {
        None
    };
    let renderer_logger = diagnostics.logger(minimium_diagnostics::DomainId::Renderer);
    drop(logger);
    let earth_package_dir =
        minimium_platform::app_paths::AppPaths::resolve_path(&options.earth_package_dir)?;
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
        Instant::now() + Duration::from_millis(16),
    ));
    let mut app = App {
        window: None,
        renderer: None,
        world_map: Some(map),
        world_catalog: Some(world_catalog),
        world_manifests: Some(world_manifests),
        profile_id,
        options,
        data_dir,
        cache_dir,
        log_dir,
        earth_package_dir,
        parent_run_id,
        diagnostic_logger,
        renderer_logger,
        localizer,
        _diagnostics: diagnostics,
        startup_error: None,
        modifiers: winit::keyboard::ModifiersState::default(),
        dev_input,
    };
    let event_result = event_loop.run_app(&mut app);
    if let Some(window) = &app.window {
        window.set_visible(false);
    }
    let shutdown_deadline = Instant::now() + Duration::from_secs(30);
    let cleanup_result = app.renderer.as_mut().map_or(Ok(()), |renderer| {
        renderer.finish_on_exit(shutdown_deadline)
    });
    let startup_error = app.startup_error.take();
    drop(app.renderer.take());
    drop(app.diagnostic_logger);
    drop(app.renderer_logger);
    drop(app.localizer);
    let diagnostics_result = app._diagnostics.shutdown_until(shutdown_deadline);
    event_result?;
    cleanup_result?;
    diagnostics_result?;
    if let Some(error) = startup_error {
        return Err(error.into());
    }
    Ok(())
}
