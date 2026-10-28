use super::*;
use crate::messages::ClientText as Text;

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    world_map: Option<WorldMap>,
    profile_id: PlayerProfileId,
    options: crate::options::ClientOptions,
    data_dir: PathBuf,
    startup_error: Option<String>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = WindowAttributes::default()
            .with_title(Text::WindowTitle.to_string())
            .with_inner_size(PhysicalSize::new(1280, 720));
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
        match pollster::block_on(Renderer::new(
            Arc::clone(&window),
            world_map,
            self.profile_id,
            self.data_dir.clone(),
            self.options.map_path.clone(),
            self.options.threads,
        )) {
            Ok(mut renderer) => {
                if !self.options.demo {
                    if let Some(address) = &self.options.connect {
                        renderer.start_remote_attempt(address);
                    } else {
                        renderer.start_local_attempt(renderer.threads);
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
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if code == KeyCode::F1 && event.state == ElementState::Pressed && !event.repeat
                    {
                        renderer.console.open = !renderer.console.open;
                        let _ = window.set_cursor_grab(CursorGrabMode::None);
                        window.set_cursor_visible(true);
                        renderer.cursor_locked = false;
                        renderer.last_cursor = None;
                        renderer.keys.clear();
                        renderer.jump_pending = false;
                        return;
                    }
                    if renderer.console.open {
                        if event.state == ElementState::Pressed {
                            match code {
                                KeyCode::Escape => renderer.console.open = false,
                                KeyCode::Enter => {
                                    if super::controller::execute_console_input(renderer) {
                                        event_loop.exit();
                                    }
                                }
                                KeyCode::Backspace => renderer.console.backspace(),
                                KeyCode::ArrowUp => renderer.console.history_up(),
                                KeyCode::ArrowDown => renderer.console.history_down(),
                                KeyCode::Tab => renderer.console.complete_input(),
                                _ => {
                                    if let Some(text) = event.text.as_ref() {
                                        renderer.console.append_text(text.as_str());
                                    }
                                }
                            }
                        }
                        return;
                    }
                    if code == KeyCode::Escape && event.state == ElementState::Pressed {
                        let _ = window.set_cursor_grab(CursorGrabMode::None);
                        window.set_cursor_visible(true);
                        renderer.cursor_locked = false;
                        renderer.last_cursor = None;
                    } else if event.state == ElementState::Pressed {
                        renderer.keys.insert(code);
                        if code == KeyCode::Space && !event.repeat {
                            renderer.jump_pending = true;
                        }
                    } else {
                        renderer.keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if !renderer.console.open {
                    if renderer.cursor_locked {
                        renderer.queue_fire();
                    } else if window.set_cursor_grab(CursorGrabMode::Locked).is_ok()
                        || window.set_cursor_grab(CursorGrabMode::Confined).is_ok()
                    {
                        window.set_cursor_visible(false);
                        renderer.cursor_locked = true;
                        renderer.last_cursor = None;
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if renderer.cursor_locked && !renderer.console.open {
                    if let Some((x, y)) = renderer.last_cursor {
                        renderer.yaw += ((position.x - x) as f32) * 0.0025;
                        renderer.pitch -= ((position.y - y) as f32) * 0.0025;
                        renderer.pitch = renderer.pitch.clamp(-1.5, 1.5);
                    }
                }
                renderer.last_cursor = Some((position.x, position.y));
            }
            WindowEvent::RedrawRequested => {
                let started = Instant::now();
                let interval = started.saturating_duration_since(renderer.last_redraw);
                renderer.last_redraw = started;
                renderer.update();
                let draws = match renderer.render() {
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
                    .frame_stats
                    .record_frame(interval, started.elapsed(), draws);
                if draws > 0 {
                    renderer.fps_frames += 1;
                }
                let sample_time = renderer.fps_time.elapsed().as_secs_f64();
                if sample_time >= 1.0 {
                    let fps = renderer.fps_frames as f64 / sample_time;
                    renderer.fps_frames = 0;
                    renderer.fps_time = Instant::now();
                    window.set_title(
                        &Text::WindowTitleStatus {
                            fps,
                            health: renderer.health,
                        }
                        .to_string(),
                    );
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
                if renderer.cursor_locked && !renderer.console.open {
                    renderer.yaw += (delta.0 as f32) * 0.0025;
                    renderer.pitch -= (delta.1 as f32) * 0.0025;
                    renderer.pitch = renderer.pitch.clamp(-1.5, 1.5);
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
    let map = if let Some(path) = map_path.as_deref() {
        minimium_assets::load_map(path).map_err(|error| {
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .to_string()
        })?
    } else {
        minimium_assets::parse_map(SAMPLE_MAP)
            .map_err(|error| Text::BundledMapInvalid(error.to_string()).to_string())?
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
        profile_id,
        options,
        data_dir,
        startup_error: None,
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
