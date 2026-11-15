use super::*;
use crate::messages::ClientText as Text;

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    worlds: Option<crate::bootstrap::InitialWorlds>,
    connection: Option<crate::bootstrap::ClientConnection>,
    options: crate::options::ClientOptions,
    data_dir: PathBuf,
    log_dir: PathBuf,
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
        let worlds = self.worlds.take().unwrap_or_else(|| {
            panic!(
                "{}",
                Text::RendererAlreadyInitialized.localized(&self.localizer, self.options.locale)
            )
        });
        match pollster::block_on(Renderer::new(init::RendererSetup {
            window: Arc::clone(&window),
            worlds,
            connection: self
                .connection
                .take()
                .expect("idle client connection is available"),
            diagnostic_logger: self.diagnostic_logger.clone(),
            renderer_logger: self.renderer_logger.clone(),
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
            startup_settings: self.options.startup_settings.clone(),
            config_file_settings: self.options.config_file_settings.clone(),
            require_earth: self.options.require_earth,
            demo: self.options.demo,
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
                super::window_events::mouse_input(renderer, window, event_loop, state, button)
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
                let submerged = renderer
                    .application
                    .game
                    .gameplay
                    .prediction
                    .submerged_fraction();
                let draws = match renderer.gpu.render(&scene, submerged) {
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
    let crate::bootstrap::ClientBootstrap {
        diagnostics,
        logger: diagnostic_logger,
        localizer,
        paths,
        worlds,
        connection,
    } = crate::bootstrap::ClientBootstrap::new(&options)?;
    let data_dir = paths.data_dir.clone();
    let log_dir = paths.log_dir.clone();
    let parent_run_id = diagnostics.run_id().to_owned();
    let dev_input = if options.dev_stdin {
        let (sender, receiver) = std::sync::mpsc::sync_channel(32);
        crate::spawn_console_input_reader(sender, diagnostic_logger.clone());
        Some(receiver)
    } else {
        None
    };
    let renderer_logger = diagnostics.logger(minimium_diagnostics::DomainId::Renderer);
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
        Instant::now() + Duration::from_millis(16),
    ));
    let mut app = App {
        window: None,
        renderer: None,
        worlds: Some(worlds),
        connection: Some(connection),
        options,
        data_dir,
        log_dir,
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
