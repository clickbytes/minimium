use super::*;
use crate::messages::ClientText as Text;

impl Renderer {
    pub(super) fn update(&mut self) -> environment::PreparedEnvironmentFrame {
        if self.overview_open && self.overview_instance != Some(self.current_instance) {
            self.overview_open = false;
            self.gpu.overview_active = false;
        }
        let now = Instant::now();
        let dt = now
            .duration_since(self.metrics.last_frame)
            .as_secs_f32()
            .min(0.05);
        self.metrics.last_frame = now;
        let direction = if self.game.sectors_ready && !self.overview_open {
            self.input.movement(
                self.ui.console.open,
                self.game.character,
                self.is_earth_world(),
            )
        } else {
            [0.0; 2]
        };
        self.game.physics_accumulator += dt;
        while self.game.physics_accumulator >= 0.05 {
            let mut input = CharacterInput {
                direction: if self.game.sectors_ready && !self.overview_open {
                    direction
                } else {
                    [0.0; 2]
                },
                jump: self.input.jump_pending && self.game.sectors_ready && !self.overview_open,
            };
            self.input.jump_pending = false;
            let previous_state = self.game.character;
            let previous_mm = self.game.position_mm;
            self.game.predict(input);
            if self
                .world_manifests
                .get(&self.current_instance)
                .is_some_and(|manifest| {
                    let position_mm = self.game.position_mm.unwrap_or_else(|| {
                        minimium_core::world::Position3Mm(
                            self.game
                                .character
                                .position
                                .map(|value| (f64::from(value) * 1000.0).round() as i64),
                        )
                    });
                    !minimium_procgen::can_traverse_surface(*manifest, position_mm)
                })
            {
                self.game.character = previous_state;
                self.game.position_mm = previous_mm;
                self.game.character.velocity = [0.0; 3];
                input = CharacterInput::default();
            }
            if !self.refresh_active_sector_scene() {
                self.game.character = previous_state;
                self.game.position_mm = previous_mm;
                input = CharacterInput::default();
            }
            self.input.input_keepalive += 0.05;
            let moving = input.direction != [0.0, 0.0];
            let input_changed = input.direction != self.input.last_sent_direction;
            if self
                .connection
                .session()
                .is_some_and(|session| session.state() == SessionState::Connected)
                && (input.jump || input_changed || (moving && self.input.input_keepalive >= 0.1))
            {
                if let Some(session) = self.connection.session_mut() {
                    let result = session.queue(&ClientMessage::Move {
                        direction: input.direction,
                        jump: input.jump,
                    });
                    if let Err(error) = result {
                        self.fail_connection(Text::CouldNotQueueMovement(error.into()).to_string());
                    } else {
                        self.input.last_sent_direction = input.direction;
                        self.input.input_keepalive = 0.0;
                    }
                }
            }
            self.game.physics_accumulator -= 0.05;
        }
        let origin = if self
            .world_manifests
            .get(&self.current_instance)
            .is_some_and(|manifest| manifest.planet().is_some())
        {
            self.game
                .character
                .position
                .map(|coordinate| (coordinate / 256.0).round() * 256.0)
        } else {
            [0.0; 3]
        };
        if self.gpu.set_render_origin(origin) {
            self.last_render_position = [f32::INFINITY; 3];
            self.game.remote_geometry_dirty = true;
        }
        self.poll_connector();
        self.poll_session();
        self.poll_globe();
        self.update_globe_detail();
        if self.refresh_active_sector_scene() && self.teleport_loading {
            self.teleport_loading = false;
            self.gpu.scene_visible = true;
            self.connection_status = "Earth: przeniesiono".into();
        }
        self.advance_pending_transitions();
        self.update_environment_frame();
        self.update_remote_player_rendering(Instant::now());

        let (view, far) = if self.overview_open {
            self.overview_camera()
        } else {
            let direction =
                camera_direction(self.game.character, self.input.yaw, self.input.pitch).normalize();
            let up = glam::Vec3::from_array(
                character_frame(self.game.character.gravity, self.game.character.position).0,
            );
            let camera_position = glam::Vec3::from_array(self.game.character.position) + up * 1.7;
            let far = if self
                .world_manifests
                .get(&self.current_instance)
                .is_some_and(|manifest| manifest.topology() == WorldTopology::Planet)
            {
                let radius = self
                    .world_manifests
                    .get(&self.current_instance)
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
        let render_origin = if self.overview_open {
            glam::Vec3::ZERO
        } else {
            glam::Vec3::from_array(self.gpu.render_origin)
        };
        let camera_eye_world = (camera_eye_local + render_origin).as_dvec3().to_array();
        // The overview camera is millions of metres from the globe. A 5 cm
        // near plane rounds its depth to 1.0 in f32 and hides the surface.
        let near = if self.overview_open
            && self
                .world_manifests
                .get(&self.current_instance)
                .is_some_and(|manifest| manifest.topology() == WorldTopology::Planet)
        {
            let radius = self
                .world_manifests
                .get(&self.current_instance)
                .map(|manifest| {
                    minimium_procgen::planet_surface_for(*manifest).radius_mm() as f32 / 1000.0
                })
                .unwrap_or(10_000.0);
            radius * ((self.overview_zoom - 1.0) * 0.08).max(0.001)
        } else {
            0.05
        };
        let view = if self.is_earth_world() {
            glam::Mat4::from_scale(glam::Vec3::new(-1.0, 1.0, 1.0)) * view
        } else {
            view
        };
        let projection = glam::Mat4::perspective_rh(
            70f32.to_radians(),
            self.gpu.size.width as f32 / self.gpu.size.height as f32,
            near,
            far,
        );
        let game_planet_radius_m = self
            .world_manifests
            .get(&self.current_instance)
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
        let render_origin_m = if self.overview_open {
            [0.0; 3]
        } else {
            self.gpu.render_origin.map(f64::from)
        };
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
                environment::PreparedEnvironmentFrame::new(self.render_environment_frame, None)
            }
        };

        let vertex_count = {
            let vertices = self.ui.console.build_vertices(
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
                },
            );
            let count = vertices.len() as u32;
            if count > 0 {
                self.gpu.queue.write_buffer(
                    &self.gpu.ui_vertex_buffer,
                    0,
                    bytemuck::cast_slice(vertices),
                );
            }
            count
        };
        self.gpu.ui_vertex_count = vertex_count;
        prepared_environment
    }
}

impl GameplayView {
    fn predict(&mut self, input: CharacterInput) {
        self.collision_query
            .fill(&self.collision_index, self.character, 0.05);
        if let Some(position_mm) = &mut self.position_mm {
            minimium_core::world::step_character_mm(
                &mut self.character,
                position_mm,
                input,
                self.collision_query.candidates(),
                self.collision_query.surface_candidates(),
                0.05,
            );
        } else {
            step_character(
                &mut self.character,
                input,
                self.collision_query.candidates(),
                self.collision_query.surface_candidates(),
                0.05,
            );
        }
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
    match manifest.topology() {
        WorldTopology::Planet => CharacterGravity::Radial { center: [0.0; 3] },
        WorldTopology::Dungeon => CharacterGravity::FixedY,
    }
}
