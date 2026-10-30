use super::*;
use crate::messages::ClientText as Text;

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    world_map: Option<WorldMap>,
    world_catalog:
        Option<std::collections::HashMap<minimium_core::world::WorldInstanceId, WorldMap>>,
    world_manifests: Option<
        std::collections::HashMap<
            minimium_core::world::WorldInstanceId,
            minimium_core::world::WorldManifest,
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
        let attributes = WindowAttributes::default()
            .with_title(Text::WindowTitle.to_string())
            .with_inner_size(PhysicalSize::new(1920, 1080));
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                self.startup_error =
                    Some(Text::WindowCreationFailed(error.to_string()).to_string());
                event_loop.exit();
                return;
            }
        };
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
            data_dir: self.data_dir.clone(),
            map_path: self.options.map_path.clone(),
            world_definition_path: self.options.world_definition_path.clone(),
            threads: self.options.threads,
            sky_quality: self.options.sky_quality,
            require_earth: self.options.require_earth,
            demo: self.options.demo,
        })) {
            Ok(mut renderer) => {
                if !self.options.demo {
                    if let Some(address) = &self.options.connect {
                        renderer.start_remote_attempt(address);
                    } else {
                        renderer.start_local_attempt(renderer.settings.threads);
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
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::F1 && event.state == ElementState::Pressed && !event.repeat
                    {
                        renderer.ui.console.open = !renderer.ui.console.open;
                        let _ = window.set_cursor_grab(CursorGrabMode::None);
                        window.set_cursor_visible(true);
                        renderer.input.cursor_locked = false;
                        if !renderer.overview_open {
                            renderer.input.last_cursor = None;
                        }
                        renderer.input.keys.clear();
                        renderer.input.jump_pending = false;
                        return;
                    }
                    if renderer.ui.console.open {
                        if event.state == ElementState::Pressed {
                            match code {
                                KeyCode::Escape => renderer.ui.console.open = false,
                                KeyCode::Enter => {
                                    if super::controller::execute_console_input(renderer) {
                                        event_loop.exit();
                                    }
                                }
                                _ => super::console_events::key(
                                    &mut renderer.ui.console,
                                    code,
                                    event.text.as_ref().map(|text| text.as_str()),
                                    self.modifiers,
                                ),
                            }
                        }
                        return;
                    }
                    if code == KeyCode::KeyM
                        && event.state == ElementState::Pressed
                        && !event.repeat
                    {
                        renderer.toggle_overview();
                        let _ = window.set_cursor_grab(CursorGrabMode::None);
                        window.set_cursor_visible(true);
                        renderer.input.cursor_locked = false;
                        renderer.input.last_cursor = None;
                        renderer.input.keys.clear();
                        renderer.input.jump_pending = false;
                        return;
                    }
                    if renderer.overview_open {
                        if code == KeyCode::Escape && event.state == ElementState::Pressed {
                            renderer.toggle_overview();
                        }
                        return;
                    }
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        renderer.connection.cancel_transition();
                        let _ = window.set_cursor_grab(CursorGrabMode::None);
                        window.set_cursor_visible(true);
                        renderer.input.cursor_locked = false;
                        renderer.input.last_cursor = None;
                    } else if event.state == ElementState::Pressed {
                        renderer.input.keys.insert(code);
                        if code == KeyCode::Space && !event.repeat {
                            renderer.input.jump_pending = true;
                        }
                        if code == KeyCode::KeyE && !event.repeat {
                            renderer.queue_portal();
                        }
                    } else {
                        renderer.input.keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if renderer.ui.console.open {
                    if let Some((x, y)) = renderer.input.last_cursor {
                        let size = window.inner_size();
                        renderer.ui.console.editor.dragging =
                            renderer
                                .ui
                                .console
                                .input_hit(size.width, size.height, x, y, false);
                    }
                    return;
                }
                if renderer.overview_open && !renderer.ui.console.open {
                    if renderer.overview_uses_zoom() {
                        if let Some((x, y)) = renderer.input.last_cursor {
                            let size = window.inner_size();
                            if crate::console::slider_hit(size.width, size.height, x, y) {
                                renderer.overview_slider_dragging = true;
                                renderer.overview_dragged = true;
                                renderer.set_overview_zoom(crate::console::slider_value_at_y(
                                    size.height,
                                    y,
                                ));
                                return;
                            }
                        }
                    }
                    renderer.overview_dragging = true;
                    renderer.overview_dragged = false;
                    renderer.overview_drag_distance = 0.0;
                    return;
                }
                if !renderer.ui.console.open && !renderer.overview_open {
                    if renderer.input.cursor_locked {
                        renderer.queue_fire();
                    } else if window.set_cursor_grab(CursorGrabMode::Locked).is_ok()
                        || window.set_cursor_grab(CursorGrabMode::Confined).is_ok()
                    {
                        window.set_cursor_visible(false);
                        renderer.input.cursor_locked = true;
                        renderer.input.last_cursor = None;
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                renderer.ui.console.editor.dragging = false;
                let slider_dragged = std::mem::take(&mut renderer.overview_slider_dragging);
                if renderer.overview_open
                    && !renderer.ui.console.open
                    && !renderer.overview_dragged
                    && !slider_dragged
                {
                    renderer.click_map_teleport();
                }
                renderer.overview_dragging = false;
            }
            WindowEvent::MouseWheel { delta, .. } if renderer.ui.console.open => {
                let amount = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, lines) => lines,
                    winit::event::MouseScrollDelta::PixelDelta(position) => {
                        position.y as f32 / 40.0
                    }
                };
                renderer.ui.console.scroll_lines(amount.round() as isize);
            }
            WindowEvent::MouseWheel { delta, .. } if renderer.overview_open => {
                let amount = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, lines) => lines,
                    winit::event::MouseScrollDelta::PixelDelta(position) => {
                        position.y as f32 / 80.0
                    }
                };
                if renderer.overview_uses_zoom() {
                    renderer.set_overview_zoom(renderer.overview_zoom * (1.0 - amount * 0.1));
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if renderer.overview_slider_dragging {
                    renderer.set_overview_zoom(crate::console::slider_value_at_y(
                        window.inner_size().height,
                        position.y,
                    ));
                }
                if renderer.ui.console.open && renderer.ui.console.editor.dragging {
                    let size = window.inner_size();
                    renderer.ui.console.input_hit(
                        size.width,
                        size.height,
                        position.x,
                        position.y,
                        true,
                    );
                }
                if renderer.overview_open && renderer.overview_dragging && !renderer.ui.console.open
                {
                    if let Some((x, y)) = renderer.input.last_cursor {
                        renderer.overview_drag_distance +=
                            ((position.x - x).powi(2) + (position.y - y).powi(2)).sqrt();
                        renderer.overview_dragged |= renderer.overview_drag_distance > 5.0;
                        let horizontal = if renderer.is_earth_world() { 1.0 } else { -1.0 };
                        renderer.overview_yaw += (position.x - x) as f32 * 0.005 * horizontal;
                        renderer.overview_pitch = (renderer.overview_pitch
                            + (position.y - y) as f32 * 0.005)
                            .clamp(-1.4, 1.4);
                    }
                }
                if renderer.input.cursor_locked
                    && !renderer.ui.console.open
                    && !renderer.overview_open
                {
                    if let Some((x, y)) = renderer.input.last_cursor {
                        let horizontal = if renderer.is_earth_world() { -1.0 } else { 1.0 };
                        renderer.input.yaw += ((position.x - x) as f32) * 0.0025 * horizontal;
                        renderer.input.pitch -= ((position.y - y) as f32) * 0.0025;
                        renderer.input.pitch = renderer.input.pitch.clamp(-1.5, 1.5);
                    }
                }
                renderer.input.last_cursor = Some((position.x, position.y));
            }
            WindowEvent::RedrawRequested => {
                let started = Instant::now();
                let interval = started.saturating_duration_since(renderer.metrics.last_redraw);
                renderer.metrics.last_redraw = started;
                let prepared_environment = renderer.update();
                let player_position_m = renderer.game.position_mm.map_or_else(
                    || renderer.game.character.position.map(f64::from),
                    |position| position.0.map(|value| value as f64 / 1000.0),
                );
                let scene = super::scene_bridge::RenderSceneSnapshot::new(
                    &prepared_environment,
                    renderer.world_catalog.get(&renderer.game.current_instance),
                    player_position_m,
                    renderer.game.current_instance,
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
                            let [x, y, z] = renderer.game.character.position.map(f64::from);
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
                            health: renderer.game.health
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
        if let DeviceEvent::MouseMotion { delta } = event {
            if let Some(renderer) = &mut self.renderer {
                if renderer.input.cursor_locked
                    && !renderer.ui.console.open
                    && !renderer.overview_open
                {
                    let horizontal = if renderer.is_earth_world() { -1.0 } else { 1.0 };
                    renderer.input.yaw += (delta.0 as f32) * 0.0025 * horizontal;
                    renderer.input.pitch -= (delta.1 as f32) * 0.0025;
                    renderer.input.pitch = renderer.input.pitch.clamp(-1.5, 1.5);
                }
            }
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
        let map = minimium_assets::load_map(path).map_err(|error| {
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .to_string()
        })?;
        let mut catalog = std::collections::HashMap::new();
        let instance = minimium_core::world::WorldInstanceId::new(1)
            .expect("legacy world instance is nonzero");
        catalog.insert(instance, map.clone());
        let manifests = std::collections::HashMap::from([(instance, legacy_manifest(1, &map))]);
        (map, catalog, manifests)
    } else if let Some(path) = world_definition_path.as_deref() {
        let definition = minimium_assets::load_world_definition(path).map_err(|error| {
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .to_string()
        })?;
        let map = minimium_procgen::materialize_dungeon_map(definition.dungeon())
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let instance = minimium_core::world::WorldInstanceId::new(4)
            .expect("manual world instance ID is nonzero");
        let mut catalog = std::collections::HashMap::from([(instance, map.clone())]);
        let mut manifests = std::collections::HashMap::from([(instance, definition.manifest())]);
        let legacy_map = minimium_assets::parse_map(crate::SAMPLE_MAP)
            .map_err(|error| Text::BundledMapInvalid(error).to_string())?;
        let legacy_instance =
            minimium_core::world::WorldInstanceId::new(3).expect("legacy instance ID is nonzero");
        manifests.insert(legacy_instance, legacy_manifest(3, &legacy_map));
        catalog.insert(legacy_instance, legacy_map);
        (map, catalog, manifests)
    } else {
        let worlds = minimium_procgen::generate_demo_worlds()
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
            minimium_core::world::WorldInstanceId::new(3).expect("legacy ID is nonzero");
        manifests.insert(legacy_instance, legacy_manifest(3, &legacy_map));
        catalog.insert(legacy_instance, legacy_map);
        let map = catalog
            .get(&minimium_core::world::WorldInstanceId::new(1).expect("planet ID is nonzero"))
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

fn legacy_manifest(id: u128, map: &WorldMap) -> minimium_core::world::WorldManifest {
    use minimium_core::world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, WorldContentRevision,
        WorldDefinitionId, WorldManifest, WorldSeed, WorldTopology,
    };
    let generation = GenerationSpec::new(
        WorldSeed::new(0),
        GenerationRevision::new(1).expect("legacy generator revision is nonzero"),
        1,
    )
    .expect("legacy position quantum is nonzero");
    WorldManifest::new(
        WorldDefinitionId::new(id).expect("legacy definition ID is nonzero"),
        WorldTopology::Dungeon,
        generation,
        WorldContentRevision::new(1).expect("legacy content revision is nonzero"),
        GeneratorDigest::legacy_map_fingerprint(map.fingerprint()),
    )
}
