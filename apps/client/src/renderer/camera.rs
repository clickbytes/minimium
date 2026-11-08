use super::*;
use crate::messages::ClientText as Text;

impl Renderer {
    pub(super) fn update(&mut self) -> environment::PreparedEnvironmentFrame {
        if self.application.overview().is_open()
            && self.application.overview().instance()
                != Some(self.application.game.current_instance)
        {
            self.application.overview_close();
            self.gpu.overview_active = false;
        }
        self.gpu.world_geometry.begin_frame();
        let now = Instant::now();
        let dt = now
            .duration_since(self.metrics.last_frame)
            .as_secs_f32()
            .min(minimium_simulation::SIMULATION_STEP_SECONDS);
        self.metrics.last_frame = now;
        let direction = if (self.application.game.gameplay.sectors_ready
            || self.application.connection.movement.can_fly)
            && !self.application.overview().is_open()
        {
            self.input.movement(
                &self.application.input,
                self.application.ui.console.open,
                self.application.game.gameplay.prediction.character,
                self.is_earth_world(),
            )
        } else {
            [0.0; 2]
        };
        self.application.game.accumulate_prediction_time(dt);
        let manifest = self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
            .copied();
        while let Some(predicted) = self.application.game.predict_next(
            direction,
            if self.application.ui.console.open
                || self.application.overview().is_open()
                || !self.application.connection.flight_enabled
            {
                0.0
            } else {
                u8::from(
                    self.application
                        .input
                        .is_active(crate::input::InputAction::MoveUp)
                        || self
                            .application
                            .input
                            .is_active(crate::input::InputAction::Jump),
                ) as f32
                    - u8::from(
                        self.application
                            .input
                            .is_active(crate::input::InputAction::MoveDown),
                    ) as f32
            },
            self.application.connection.movement,
            self.input.jump_pending,
            (self.application.game.gameplay.sectors_ready
                || self.application.connection.movement.can_fly)
                && !self.application.overview().is_open(),
            manifest,
            self.application
                .connection
                .session()
                .and_then(|session| session.prepared_geography()),
        ) {
            let mut input = predicted.input;
            self.input.jump_pending = false;
            if !self.refresh_active_sector_scene() && !self.application.connection.movement.can_fly
            {
                self.application.game.gameplay.prediction.character = predicted.previous_state;
                self.application.game.gameplay.prediction.position_mm = predicted.previous_mm;
                input = MovementIntent::default();
            }
            if self
                .application
                .connection
                .session()
                .is_some_and(|session| session.state() == SessionState::Connected)
            {
                let policy = self.application.connection.movement;
                let collision_revision = self.application.game.gameplay.collision_revision;
                if let Some(session) = self.application.connection.session_mut() {
                    let result =
                        session.queue_world_input(minimium_protocol::ClientWorldInput::Motion(
                            minimium_protocol::MovementSample::new(input),
                        ));
                    match result {
                        Err(error) => self.fail_connection(
                            Text::CouldNotQueueMovement(error.into())
                                .localized(&self.localizer, self.application.settings.locale),
                        ),
                        Ok(Some(sample)) => {
                            self.application.game.gameplay.prediction.record_input(
                                sample,
                                policy,
                                collision_revision,
                            );
                        }
                        Ok(None) => {}
                    }
                }
            }
        }
        let origin = if self.application.overview().is_open() {
            self.gpu.render_origin
        } else if self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
            .is_some_and(|manifest| manifest.planet().is_some())
        {
            let position = self.application.game.gameplay.prediction.character.position;
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
            self.shadow_state
                .invalidate(shadows::ShadowInvalidReason::StaleContext);
            self.last_render_position = [f32::INFINITY; 3];
            self.gameplay_presentation.remote_geometry_dirty = true;
        }
        self.poll_connector();
        self.poll_session();
        if self.refresh_active_sector_scene()
            && self.application.is_loading()
            && self.last_render_position.iter().all(|p| p.is_finite())
        {
            self.application.set_loading(false);
            self.gpu.scene_visible = true;
            self.application.status = crate::application::ClientStatus::Status(
                minimium_localization::UiStatus::EarthMoved,
            );
        }
        self.advance_pending_transitions();
        self.update_environment_frame();
        self.send_sky_view(now);
        self.flush_pending_bolid(now);
        self.update_remote_player_rendering(Instant::now());

        let visual_offset = self
            .application
            .game
            .gameplay
            .prediction
            .take_visual_offset(dt);
        let (view, far) = if self.application.overview().is_open() {
            let (view, far) = self.overview_camera();
            (
                view * glam::Mat4::from_translation(glam::Vec3::from_array(self.gpu.render_origin)),
                far,
            )
        } else {
            let direction = camera_direction(
                self.application.game.gameplay.prediction.character,
                self.input.yaw,
                self.input.pitch,
            )
            .normalize();
            let up = glam::Vec3::from_array(
                character_frame(
                    self.application.game.gameplay.prediction.character.gravity,
                    self.application.game.gameplay.prediction.character.position,
                )
                .0,
            );
            let camera_position = glam::Vec3::from_array(
                self.application.game.gameplay.prediction.character.position,
            ) + glam::Vec3::from_array(visual_offset)
                + up * 1.7;
            let far = if self
                .application
                .game
                .world_manifests
                .get(&self.application.game.current_instance)
                .is_some_and(|manifest| manifest.topology() == WorldTopology::Planet)
            {
                let radius = self
                    .application
                    .game
                    .world_manifests
                    .get(&self.application.game.current_instance)
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
            let local_camera = if let Some(position_mm) =
                self.application.game.gameplay.prediction.position_mm
            {
                glam::Vec3::from_array(std::array::from_fn(|axis| {
                    (position_mm.0[axis] as f64 / 1000.0 - f64::from(self.gpu.render_origin[axis]))
                        as f32
                })) + glam::Vec3::from_array(visual_offset)
                    + up * 1.7
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
        let near = if self.application.overview().is_open()
            && self
                .application
                .game
                .world_manifests
                .get(&self.application.game.current_instance)
                .is_some_and(|manifest| manifest.topology() == WorldTopology::Planet)
        {
            let radius = self
                .application
                .game
                .world_manifests
                .get(&self.application.game.current_instance)
                .map(|manifest| {
                    minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0
                })
                .unwrap_or(10_000.0);
            radius * ((self.application.overview().zoom() - 1.0) * 0.08).max(0.001)
        } else {
            let terrain_near = self.gpu.world_geometry.surface_near(
                self.application.game.current_instance,
                &self
                    .gpu
                    .world_geometry
                    .publication()
                    .map_or_else(Vec::new, |publication| publication.keys.clone()),
                camera_eye_world,
            );
            self.application
                .game
                .gameplay
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
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
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
            kind: if self.application.overview().is_open() {
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
        let player_position = self
            .application
            .game
            .gameplay
            .prediction
            .character
            .position
            .map(f64::from);
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
                        .application
                        .game
                        .world_manifests
                        .get(&self.application.game.current_instance)
                        .and_then(|m| m.planet())
                        .map_or(0.0, |p| p.sea_level_mm as f64 / 1000.0)
            } else {
                0.0
            },
            self.application
                .game
                .world_manifests
                .get(&self.application.game.current_instance)
                .and_then(|m| m.planet())
                .filter(|p| p.mode == 1)
                .map_or(1.0, |p| p.height_scale_permille as f64 / 1000.0),
            self.sky_quality.effective == minimium_renderer::quality::SkyQuality::Standard,
            self.environment_frame
                .render_world_utc_ms
                .map_or(0.0, |t| t as f64 / 1000.0),
        );

        let shadow_prepare_started = Instant::now();
        self.prepared_bolids = self.prepare_bolids();
        self.prepare_shadow_frame(&prepared_environment.frame, player_position);
        self.gpu.last_shadow_prepare_cpu = Some(shadow_prepare_started.elapsed());

        let celestial_labels = if self.show_celestial_names {
            celestial_labels::prepare(&prepared_environment.frame, self.sky_catalogue.as_ref())
        } else {
            Vec::new()
        };
        let connection_status = self
            .application
            .status
            .format(&self.localizer, self.application.settings.locale);
        self.application.ui.console.build_vertices(
            self.gpu.size.width,
            self.gpu.size.height,
            self.application.game.gameplay.health,
            self.input.cursor_locked && !self.application.ui.console.open,
            self.gameplay_presentation
                .hit_confirmation_until
                .is_some_and(|until| Instant::now() < until),
            crate::console::Overlay {
                status: self
                    .application
                    .settings
                    .require_earth
                    .then_some(connection_status.as_str()),
                overview_zoom: self
                    .overview_uses_zoom()
                    .then_some(self.application.overview().zoom()),
                celestial_labels: &celestial_labels,
            },
        );
        let ui_vertices = self.application.ui.console.take_vertices();
        prepared_environment.with_ui_vertices(ui_vertices)
    }
}

impl InputState {
    fn movement(
        &self,
        mapper: &crate::input::InputMapper,
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
            if mapper.is_active(crate::input::InputAction::MoveForward) {
                movement += forward;
            }
            if mapper.is_active(crate::input::InputAction::MoveBackward) {
                movement -= forward;
            }
            if mapper.is_active(crate::input::InputAction::MoveRight) {
                movement += right;
            }
            if mapper.is_active(crate::input::InputAction::MoveLeft) {
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
