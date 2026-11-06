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
    startup_error: Option<String>,
    modifiers: winit::keyboard::ModifiersState,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = super::window_mode::attributes(event_loop, self.options.windowed)
            .with_title(Text::WindowTitle.to_string());
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.startup_error =
                    Some(Text::WindowCreationFailed(error.to_string()).to_string());
                event_loop.exit();
                return;
            }
        };
        if self.options.windowed {
            super::window_mode::fit_windowed(&window, event_loop);
        }
        let world_map = self
            .world_map
            .take()
            .unwrap_or_else(|| panic!("{}", Text::RendererAlreadyInitialized));
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
            map_path: self.options.map_path.clone(),
            world_definition_path: self.options.world_definition_path.clone(),
            local_server_config: self
                .options
                .local_server_launch_config(self.data_dir.clone(), None)
                .expect("validated local server launch options"),
            sky_quality: self.options.sky_quality,
            startup_fov: self.options.startup_fov,
            startup_mouse_sensitivity: self.options.startup_mouse_sensitivity,
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
                self.startup_error =
                    Some(Text::GraphicsInitializationFailed(error.to_string()).to_string());
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
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } => {
                super::window_events::keyboard(renderer, window, event_loop, event, self.modifiers)
            }
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
                let client_view = renderer.game.view();
                prepared_environment = prepared_environment.with_client_view(&client_view);
                let scene = super::scene_bridge::RenderSceneSnapshot::new(
                    &prepared_environment,
                    renderer.world_catalog.get(&client_view.world_instance),
                    renderer.gpu.active_sector_keys.clone(),
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
                        eprintln!("{}", Text::RenderFailed(format!("{error:?}")));
                        0
                    }
                };
                renderer
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
                    let coordinates =
                        if renderer.settings.require_earth && renderer.gpu.scene_visible {
                            let [x, y, z] = renderer
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
                            health: renderer.game.gameplay.health
                        },
                        renderer.connection_status,
                        coordinates,
                        renderer.environment_frame.title_suffix()
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
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        event_loop.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(16),
        ));
    }
}

pub fn run(options: crate::options::ClientOptions) -> Result<(), Box<dyn Error>> {
    let map_path = options.map_path.clone();
    let world_definition_path = options.world_definition_path.clone();
    let (map, world_catalog, world_manifests) = if let Some(path) = map_path.as_deref() {
        let map = minimium_asset_io::load_map(path).map_err(|error| {
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .to_string()
        })?;
        let mut catalog = std::collections::HashMap::new();
        let instance = minimium_world::world::WorldInstanceId::new(1)
            .expect("legacy world instance is nonzero");
        catalog.insert(instance, map.clone());
        let manifests = std::collections::HashMap::from([(instance, legacy_manifest(1, &map))]);
        (map, catalog, manifests)
    } else if let Some(path) = world_definition_path.as_deref() {
        let definition = minimium_asset_io::load_world_definition(path).map_err(|error| {
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .to_string()
        })?;
        super::validate_world_manifest(definition.manifest())?;
        let map = minimium_procgen::materialize_dungeon_map(definition.dungeon())
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let instance = minimium_world::world::WorldInstanceId::new(4)
            .expect("manual world instance ID is nonzero");
        let mut catalog = std::collections::HashMap::from([(instance, map.clone())]);
        let mut manifests = std::collections::HashMap::from([(instance, definition.manifest())]);
        let legacy_map = minimium_assets::parse_map(crate::SAMPLE_MAP)
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let legacy_instance =
            minimium_world::world::WorldInstanceId::new(3).expect("legacy instance ID is nonzero");
        manifests.insert(legacy_instance, legacy_manifest(3, &legacy_map));
        catalog.insert(legacy_instance, legacy_map);
        (map, catalog, manifests)
    } else {
        let worlds = minimium_game_sample::generate_demo_worlds()
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let mut catalog = std::collections::HashMap::new();
        let mut manifests = std::collections::HashMap::new();
        for world in worlds {
            manifests.insert(world.instance, world.manifest);
            catalog.insert(world.instance, world.map);
        }
        let legacy_map = minimium_assets::parse_map(crate::SAMPLE_MAP)
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let legacy_instance =
            minimium_world::world::WorldInstanceId::new(3).expect("legacy ID is nonzero");
        manifests.insert(legacy_instance, legacy_manifest(3, &legacy_map));
        catalog.insert(legacy_instance, legacy_map);
        let map = catalog
            .get(&minimium_world::world::WorldInstanceId::new(1).expect("planet ID is nonzero"))
            .expect("demo generator always returns a planet")
            .clone();
        (map, catalog, manifests)
    };
    let (profile_dir, data_dir) = options.paths(map.fingerprint());
    let profile_id = minimium_platform::identity_store::load_or_create(&profile_dir)?;
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
        startup_error: None,
        modifiers: winit::keyboard::ModifiersState::default(),
    };
    let event_result = event_loop.run_app(&mut app);
    let cleanup_result = app
        .renderer
        .as_mut()
        .map_or(Ok(()), Renderer::finish_on_exit);
    event_result?;
    cleanup_result?;
    if let Some(error) = app.startup_error {
        return Err(error.into());
    }
    Ok(())
}
