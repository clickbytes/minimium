use super::*;
use crate::messages::ClientText as Text;

mod character;
pub(super) use character::{camera_direction, camera_direction_from_geometry};

impl DesktopHost {
    pub(super) fn step(&mut self, now: Instant) {
        if let Err(error) = self.application.validate_time(crate::host_time::sample()) {
            self.application
                .record_diagnostic_failure("client.invalid_time", &error);
            return;
        }
        if self.application.overview().is_open()
            && self.application.overview().instance()
                != Some(self.application.game_view().current_instance)
        {
            self.application.overview_close();
            self.renderer.overview_active = false;
        }
        self.update_tool_use();
        self.renderer.world_geometry.begin_frame();

        let game = self.application.game_view();
        let direction = self.input.movement(
            &self.application.input,
            self.application.ui.console.open,
            game.gameplay.prediction.character,
            self.is_earth_world(),
        );
        let vertical = u8::from(
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
            ) as f32;
        let context = self.application.game_view().session_context();
        let water = context.and_then(|context| self.water_presentation_for(context));
        let fov_degrees = self.fov_degrees;
        let published_terrain = self.impacts.published_terrain;
        let application = &mut self.application;
        let sector_cache = &mut self.sector_cache;
        let renderer = &mut self.renderer;
        let output = application.step(
            crate::application::ClientStepInput {
                now: crate::host_time::sample(),
                prediction: Some(crate::application::PredictionInput {
                    direction,
                    vertical,
                    jump_pending: self.input.jump_pending,
                }),
            },
            |request| {
                super::session::prepare_prediction_scene(
                    sector_cache,
                    renderer,
                    request,
                    water.clone(),
                    fov_degrees,
                    published_terrain,
                )
            },
        );
        let output = match output {
            Ok(output) => output,
            Err(error) => {
                self.application
                    .record_diagnostic_failure("client.invalid_time", &error);
                return;
            }
        };
        if let Some(error) = output.candidate_scene_error.as_ref() {
            self.application
                .record_diagnostic_failure("client_runtime.candidate_scene_failed", error);
        }
        self.camera_visual_offset = output.visual_offset;
        self.pending_step_disconnect = output.disconnect;
        if output.jump_consumed {
            self.input.jump_pending = false;
        }
        self.handle_step_connector(output.admin_result, output.connection);
        let disconnecting = self.pending_step_disconnect.is_some();
        self.handle_step_session_events(output.session_events, disconnecting);
        if self.pending_step_disconnect.is_some() {
            self.clear_connection();
        }
        if let Some(error) = output.movement_queue_error {
            self.fail_connection(
                Text::CouldNotQueueMovement(error.into())
                    .localized(&self.localizer, self.application.settings.locale),
            );
        }

        self.update_render_origin();
        if !output.candidate_scene_pending
            && self.refresh_active_sector_scene()
            && self.application.is_loading()
            && self.last_render_position.iter().all(|p| p.is_finite())
        {
            self.application.set_loading(false);
            self.renderer.scene_visible = true;
            self.application.status = crate::application::ClientStatus::Status(
                minimium_localization::UiStatus::EarthMoved,
            );
        }
        self.advance_pending_transitions();
        self.update_environment_frame();
        self.send_sky_view(now);
        self.flush_pending_bolid(now);
        self.update_remote_player_rendering();
    }

    pub(super) fn update(&mut self) -> environment::PreparedEnvironmentFrame {
        let render_view = self.application.desktop_render_view();
        self.renderer.held_tool_model_index = render_view.equipped_tool_model_index;
        self.renderer.held_tool_visible = self.application.game_view().gameplay.sectors_ready
            && !self.application.ui.console.open
            && !self.application.overview().is_open();
        self.update_render_origin();
        let visual_offset = self.camera_visual_offset;
        let (view, far) = if self.application.overview().is_open() {
            let (view, far) = self.overview_camera();
            (
                view * glam::Mat4::from_translation(glam::Vec3::from_array(
                    self.renderer.render_origin,
                )),
                far,
            )
        } else {
            let character = self.application.game_view().gameplay.prediction.character;
            let feet_m = self
                .application
                .game_view()
                .gameplay
                .prediction
                .position_mm
                .map_or_else(
                    || character.position.map(f64::from),
                    |position| position.0.map(|value| value as f64 / 1000.0),
                );
            let geometry = CharacterGeometry::new(feet_m, character.gravity);
            let frame = geometry
                .map(|geometry| geometry.frame())
                .unwrap_or_else(|| character::fallback_character_frame(character));
            let up = glam::Vec3::from_array(frame.up.map(|value| value as f32));
            let direction = geometry
                .map_or_else(
                    || camera_direction(character, self.input.yaw, self.input.pitch),
                    |geometry| {
                        camera_direction_from_geometry(geometry, self.input.yaw, self.input.pitch)
                    },
                )
                .normalize();
            let eye_world = geometry.map_or_else(
                || {
                    std::array::from_fn(|axis| {
                        feet_m[axis]
                            + frame.up[axis] * f64::from(minimium_world::world::PLAYER_EYE_HEIGHT)
                    })
                },
                CharacterGeometry::eye_position_m,
            );
            let camera_position = glam::Vec3::from_array(eye_world.map(|value| value as f32))
                + glam::Vec3::from_array(visual_offset);
            let far = if self
                .application
                .game_view()
                .world_manifests
                .get(&self.application.game_view().current_instance)
                .is_some_and(|manifest| manifest.topology() == WorldTopology::Planet)
            {
                let radius = self
                    .application
                    .game_view()
                    .world_manifests
                    .get(&self.application.game_view().current_instance)
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
            let local_camera = glam::Vec3::from_array(std::array::from_fn(|axis| {
                (eye_world[axis] - f64::from(self.renderer.render_origin[axis])) as f32
            })) + glam::Vec3::from_array(visual_offset);
            (glam::Mat4::look_to_rh(local_camera, direction, up), far)
        };
        let camera_eye_local = view.inverse().transform_point3(glam::Vec3::ZERO);
        let render_origin = glam::Vec3::from_array(self.renderer.render_origin);
        let camera_eye_world = (camera_eye_local + render_origin).as_dvec3().to_array();
        // The overview camera is millions of metres from the globe. A 5 cm
        // near plane rounds its depth to 1.0 in f32 and hides the surface.
        let near = if self.application.overview().is_open()
            && self
                .application
                .game_view()
                .world_manifests
                .get(&self.application.game_view().current_instance)
                .is_some_and(|manifest| manifest.topology() == WorldTopology::Planet)
        {
            let radius = self
                .application
                .game_view()
                .world_manifests
                .get(&self.application.game_view().current_instance)
                .map(|manifest| {
                    minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0
                })
                .unwrap_or(10_000.0);
            radius * ((self.application.overview().zoom() - 1.0) * 0.08).max(0.001)
        } else {
            let terrain_near = self.renderer.world_geometry.surface_near(
                self.application.game_view().current_instance,
                &self
                    .renderer
                    .world_geometry
                    .publication()
                    .map_or_else(Vec::new, |publication| publication.keys.clone()),
                camera_eye_world,
            );
            self.application
                .game_view()
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
            self.renderer.size.width as f32 / self.renderer.size.height as f32,
            near,
            far,
        );
        let game_planet_radius_m = self
            .application
            .game_view()
            .world_manifests
            .get(&self.application.game_view().current_instance)
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
        let render_origin_m = self.renderer.render_origin.map(f64::from);
        let render_view = RenderViewFrame::from_camera(RenderViewInput {
            view,
            projection,
            viewport: [self.renderer.size.width, self.renderer.size.height],
            render_origin_m,
            camera_eye_world_m: camera_eye_world,
            game_planet_radius_m,
            kind: if self.application.overview().is_open() {
                environment::EnvironmentViewKind::Overview
            } else {
                environment::EnvironmentViewKind::Surface
            },
            srgb_surface: matches!(
                self.renderer.config.format,
                wgpu::TextureFormat::Rgba8UnormSrgb | wgpu::TextureFormat::Bgra8UnormSrgb
            ),
        });
        let prepared_environment = match render_view {
            Some(view) => self.update_render_environment(view),
            None => {
                self.render_environment_frame =
                    EnvironmentFrame::unavailable(RenderViewFrame::default());
                let meteors = self.meteors.build_frame(&self.render_environment_frame);
                environment::PreparedEnvironmentFrame::new(
                    self.render_environment_frame.clone(),
                    None,
                    meteors,
                    self.cosmetic_epoch.elapsed(),
                )
            }
        };

        self.update_weather(&prepared_environment.frame);
        let player_position = self
            .application
            .game_view()
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
        self.renderer.atmosphere_gpu.upload(
            &self.renderer.device,
            &self.renderer.queue,
            prepared_environment.frame.render_view,
            if earth {
                game_planet_radius_m
                    + self
                        .application
                        .game_view()
                        .world_manifests
                        .get(&self.application.game_view().current_instance)
                        .and_then(|m| m.planet())
                        .map_or(0.0, |p| p.sea_level_mm as f64 / 1000.0)
            } else {
                0.0
            },
            self.application
                .game_view()
                .world_manifests
                .get(&self.application.game_view().current_instance)
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
        self.renderer.last_shadow_prepare_cpu = Some(shadow_prepare_started.elapsed());

        let celestial_labels = if self.show_celestial_names {
            celestial_labels::prepare(&prepared_environment.frame, self.sky_catalogue.as_ref())
        } else {
            Vec::new()
        };
        let connection_status = self
            .application
            .status
            .format(&self.localizer, self.application.settings.locale);
        let tool_overlay = self.tool_overlay(&prepared_environment.frame.render_view);
        self.application.ui.console.build_vertices(
            self.renderer.size.width,
            self.renderer.size.height,
            self.application.game_view().gameplay.health,
            self.input.cursor_locked && !self.application.ui.console.open,
            self.gameplay_presentation
                .hit_confirmation_until
                .is_some_and(|until| Instant::now() < until),
            crate::console::Overlay {
                tool: tool_overlay.as_ref(),
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

    fn update_render_origin(&mut self) {
        let origin = if self.application.overview().is_open() {
            self.renderer.render_origin
        } else if self
            .application
            .game_view()
            .world_manifests
            .get(&self.application.game_view().current_instance)
            .is_some_and(|manifest| manifest.planet().is_some())
        {
            let position = self
                .application
                .game_view()
                .gameplay
                .prediction
                .character
                .position;
            if position
                .iter()
                .zip(self.renderer.render_origin)
                .any(|(p, o)| (p - o).abs() > 4096.0)
            {
                position.map(|coordinate| (coordinate / 4096.0).round() * 4096.0)
            } else {
                self.renderer.render_origin
            }
        } else {
            [0.0; 3]
        };
        if self.renderer.set_render_origin(origin) {
            self.shadow_state
                .invalidate(shadows::ShadowInvalidReason::StaleContext);
            self.last_render_position = [f32::INFINITY; 3];
            self.gameplay_presentation.remote_geometry_dirty = true;
        }
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
