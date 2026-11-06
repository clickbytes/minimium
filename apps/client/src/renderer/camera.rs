use super::*;
use crate::messages::ClientText as Text;

impl Renderer {
    pub(super) fn update(&mut self) -> environment::PreparedEnvironmentFrame {
        if self.overview_open && self.overview_instance != Some(self.game.current_instance) {
            self.overview_open = false;
            self.gpu.overview_active = false;
        }
        self.gpu.world_geometry.begin_frame();
        let now = Instant::now();
        let dt = now
            .duration_since(self.metrics.last_frame)
            .as_secs_f32()
            .min(minimium_simulation::SIMULATION_STEP_SECONDS);
        self.metrics.last_frame = now;
        let direction =
            if (self.game.sectors_ready || self.connection.movement.fly) && !self.overview_open {
                self.input.movement(
                    self.ui.console.open,
                    self.game.character,
                    self.is_earth_world(),
                )
            } else {
                [0.0; 2]
            };
        self.game.accumulate_prediction_time(dt);
        let manifest = self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .copied();
        while let Some(predicted) = self.game.predict_next(
            direction,
            if self.ui.console.open || self.overview_open {
                0.0
            } else {
                u8::from(self.input.keys.contains(&KeyCode::Space)) as f32
                    - u8::from(
                        self.input.keys.contains(&KeyCode::ControlLeft)
                            || self.input.keys.contains(&KeyCode::ControlRight),
                    ) as f32
            },
            self.connection.movement,
            self.input.jump_pending,
            (self.game.sectors_ready || self.connection.movement.fly) && !self.overview_open,
            manifest,
            self.connection
                .session()
                .and_then(|session| session.prepared_geography()),
        ) {
            let mut input = predicted.input;
            self.input.jump_pending = false;
            if !self.refresh_active_sector_scene() && !self.connection.movement.fly {
                self.game.character = predicted.previous_state;
                self.game.position_mm = predicted.previous_mm;
                input = CharacterInput::default();
            }
            self.input.input_keepalive += minimium_simulation::SIMULATION_STEP_SECONDS;
            let moving = input.direction != [0.0, 0.0];
            let input_changed = input.direction != self.input.last_sent_direction;
            if self
                .connection
                .session()
                .is_some_and(|session| session.state() == SessionState::Connected)
                && (self.input.input_keepalive
                    >= 2.0 * minimium_simulation::SIMULATION_STEP_SECONDS
                    || input.jump
                    || input_changed
                    || (moving
                        && self.input.input_keepalive
                            >= 2.0 * minimium_simulation::SIMULATION_STEP_SECONDS))
            {
                if let Some(session) = self.connection.session_mut() {
                    let result = session.queue(&ClientMessage::Motion(input));
                    if let Err(error) = result {
                        self.fail_connection(Text::CouldNotQueueMovement(error.into()).to_string());
                    } else {
                        self.input.last_sent_direction = input.direction;
                        self.input.input_keepalive = 0.0;
                    }
                }
            }
        }
        let origin = if self.overview_open {
            self.gpu.render_origin
        } else if self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .is_some_and(|manifest| manifest.planet().is_some())
        {
            let position = self.game.character.position;
            if position
                .iter()
                .zip(self.gpu.render_origin)
                .any(|(p, o)| (p - o).abs() > 4096.0)
            {
                position.map(|coordinate| (coordinate / 4096.0).round() * 4096.0)
            } else {
                self.gpu.render_origin
            }
        } else {
            [0.0; 3]
        };
        if self.gpu.set_render_origin(origin) {
            self.last_render_position = [f32::INFINITY; 3];
            self.game.remote_geometry_dirty = true;
        }
        self.poll_connector();
        self.poll_session();
        if self.refresh_active_sector_scene()
            && self.teleport_loading
            && self.last_render_position.iter().all(|p| p.is_finite())
        {
            self.teleport_loading = false;
            self.gpu.scene_visible = true;
            self.connection_status = "Earth: przeniesiono".into();
        }
        self.advance_pending_transitions();
        self.update_environment_frame();
        self.update_remote_player_rendering(Instant::now());

        let (view, far) = if self.overview_open {
            let (view, far) = self.overview_camera();
            (
                view * glam::Mat4::from_translation(glam::Vec3::from_array(self.gpu.render_origin)),
                far,
            )
        } else {
            let direction =
                camera_direction(self.game.character, self.input.yaw, self.input.pitch).normalize();
            let up = glam::Vec3::from_array(
                character_frame(self.game.character.gravity, self.game.character.position).0,
            );
            let camera_position = glam::Vec3::from_array(self.game.character.position) + up * 1.7;
            let far = if self
                .game
                .world_manifests
                .get(&self.game.current_instance)
                .is_some_and(|manifest| manifest.topology() == WorldTopology::Planet)
            {
                let radius = self
                    .game
                    .world_manifests
                    .get(&self.game.current_instance)
                    .map(|manifest| {
                        minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0
                    })
                    .unwrap_or(10_000.0);
                (camera_position.length_squared() - radius * radius)
                    .max(0.0)
                    .sqrt()
                    .max(220.0)
                    + 10.0
            } else {
                200.0
            };
            let local_camera = if let Some(position_mm) = self.game.position_mm {
                glam::Vec3::from_array(std::array::from_fn(|axis| {
                    (position_mm.0[axis] as f64 / 1000.0 - f64::from(self.gpu.render_origin[axis]))
                        as f32
                })) + up * 1.7
            } else {
                camera_position - glam::Vec3::from_array(self.gpu.render_origin)
            };
            (glam::Mat4::look_to_rh(local_camera, direction, up), far)
        };
        let camera_eye_local = view.inverse().transform_point3(glam::Vec3::ZERO);
        let render_origin = glam::Vec3::from_array(self.gpu.render_origin);
        let camera_eye_world = (camera_eye_local + render_origin).as_dvec3().to_array();
        // The overview camera is millions of metres from the globe. A 5 cm
        // near plane rounds its depth to 1.0 in f32 and hides the surface.
        let near = if self.overview_open
            && self
                .game
                .world_manifests
                .get(&self.game.current_instance)
                .is_some_and(|manifest| manifest.topology() == WorldTopology::Planet)
        {
            let radius = self
                .game
                .world_manifests
                .get(&self.game.current_instance)
                .map(|manifest| {
                    minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0
                })
                .unwrap_or(10_000.0);
            radius * ((self.overview_zoom - 1.0) * 0.08).max(0.001)
        } else {
            let terrain_near = self.gpu.world_geometry.surface_near(
                self.game.current_instance,
                &self.gpu.active_sector_keys,
                camera_eye_world,
            );
            self.game
                .reconciliation
                .tracks()
                .iter()
                .flatten()
                .map(|track| {
                    let distance = (0..3)
                        .map(|axis| {
                            (camera_eye_world[axis]
                                - track.displayed_position_mm[axis] as f64 / 1000.0)
                                .powi(2)
                        })
                        .sum::<f64>()
                        .sqrt();
                    ((distance - 2.0).max(0.0) * 0.01).max(0.05) as f32
                })
                .fold(terrain_near, f32::min)
        };
        let view = if self.is_earth_world() {
            glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0)) * view
        } else {
            view
        };
        let projection = glam::Mat4::perspective_rh(
            self.fov_degrees.to_radians(),
            self.gpu.size.width as f32 / self.gpu.size.height as f32,
            near,
            far,
        );
        let game_planet_radius_m = self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .filter(|manifest| manifest.topology() == WorldTopology::Planet)
            .map(|manifest| {
                manifest
                    .planet()
                    .map(|definition| definition.radius_mm)
                    .unwrap_or_else(|| minimium_procgen::planet_surface_for(*manifest).radius_mm())
                    as f64
                    / 1000.0
            })
            .unwrap_or(0.0);
        let render_origin_m = self.gpu.render_origin.map(f64::from);
        let render_view = RenderViewFrame::from_camera(RenderViewInput {
            view,
            projection,
            viewport: [self.gpu.size.width, self.gpu.size.height],
            render_origin_m,
            camera_eye_world_m: camera_eye_world,
            game_planet_radius_m,
            kind: if self.overview_open {
                environment::EnvironmentViewKind::Overview
            } else {
                environment::EnvironmentViewKind::Surface
            },
            srgb_surface: matches!(
                self.gpu.config.format,
                wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
            ),
        });
        let prepared_environment = match render_view {
            Some(view) => self.update_render_environment(view),
            None => {
                self.render_environment_frame =
                    EnvironmentFrame::unavailable(RenderViewFrame::default());
                environment::PreparedEnvironmentFrame::new(
                    self.render_environment_frame.clone(),
                    None,
                )
            }
        };

        self.update_weather(&prepared_environment.frame);
        let earth = prepared_environment
            .frame
            .descriptor
            .is_some_and(|descriptor| {
                descriptor.profile() == minimium_environment::EnvironmentProfile::Earth
            });
        self.gpu.atmosphere_gpu.upload(
            &self.gpu.device,
            &self.gpu.queue,
            prepared_environment.frame.render_view,
            if earth {
                game_planet_radius_m
                    + self
                        .game
                        .world_manifests
                        .get(&self.game.current_instance)
                        .and_then(|m| m.planet())
                        .map_or(0.0, |p| p.sea_level_mm as f64 / 1000.0)
            } else {
                0.0
            },
            self.game
                .world_manifests
                .get(&self.game.current_instance)
                .and_then(|m| m.planet())
                .filter(|p| p.mode == 1)
                .map_or(1.0, |p| p.height_scale_permille as f64 / 1000.0),
            self.sky_quality.effective == minimium_renderer::quality::SkyQuality::Standard,
            self.environment_frame
                .render_world_utc_ms
                .map_or(0.0, |t| t as f64 / 1000.0),
        );

        let celestial_labels = if self.show_celestial_names {
            celestial_labels::prepare(&prepared_environment.frame, self.sky_catalogue.as_ref())
        } else {
            Vec::new()
        };
        self.ui.console.build_vertices(
            self.gpu.size.width,
            self.gpu.size.height,
            self.game.health,
            self.input.cursor_locked && !self.ui.console.open,
            self.game
                .hit_confirmation_until
                .is_some_and(|until| Instant::now() < until),
            crate::console::Overlay {
                status: self
                    .settings
                    .require_earth
                    .then_some(self.connection_status.as_str()),
                overview_zoom: self.overview_uses_zoom().then_some(self.overview_zoom),
                celestial_labels: &celestial_labels,
            },
        );
        let ui_vertices = self.ui.console.take_vertices();
        prepared_environment.with_ui_vertices(ui_vertices)
    }
}

impl InputState {
    fn movement(
        &self,
        console_open: bool,
        character: CharacterState,
        mirror_earth: bool,
    ) -> [f32; 2] {
        let (up, base_right, base_forward) = character_frame(character.gravity, character.position);
        let base_right = glam::Vec3::from_array(base_right);
        let base_forward = glam::Vec3::from_array(base_forward);
        let _up = glam::Vec3::from_array(up);
        let forward = base_forward * self.yaw.cos() + base_right * self.yaw.sin();
        let right = (base_right * self.yaw.cos() - base_forward * self.yaw.sin())
            * if mirror_earth { -1.0 } else { 1.0 };
        let mut movement = glam::Vec3::ZERO;
        if !console_open {
            if self.keys.contains(&KeyCode::KeyW) {
                movement += forward;
            }
            if self.keys.contains(&KeyCode::KeyS) {
                movement -= forward;
            }
            if self.keys.contains(&KeyCode::KeyD) {
                movement += right;
            }
            if self.keys.contains(&KeyCode::KeyA) {
                movement -= right;
            }
        }
        if movement.length_squared() > 1.0 {
            movement = movement.normalize();
        }
        if matches!(character.gravity, CharacterGravity::Radial { .. }) {
            [movement.dot(base_right), movement.dot(base_forward)]
        } else {
            [movement.x, movement.z]
        }
    }
}

pub(super) fn camera_direction(character: CharacterState, yaw: f32, pitch: f32) -> glam::Vec3 {
    let (up, right, forward) = character_frame(character.gravity, character.position);
    glam::Vec3::from_array(forward) * (yaw.cos() * pitch.cos())
        + glam::Vec3::from_array(right) * (yaw.sin() * pitch.cos())
        + glam::Vec3::from_array(up) * pitch.sin()
}

pub(super) fn gravity_for_manifest(manifest: WorldManifest) -> CharacterGravity {
    minimium_world::world::GravityPolicy::for_manifest(manifest).character_gravity()
}
