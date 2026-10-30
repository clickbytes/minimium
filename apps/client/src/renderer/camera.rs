use super::*;
use crate::messages::ClientText as Text;

impl Renderer {
    pub(super) fn update(&mut self) {
        let now = Instant::now();
        let dt = now
            .duration_since(self.metrics.last_frame)
            .as_secs_f32()
            .min(0.05);
        self.metrics.last_frame = now;
        let direction = if self.game.sectors_ready {
            self.input
                .movement(self.ui.console.open, self.game.character)
        } else {
            [0.0; 2]
        };
        self.game.physics_accumulator += dt;
        while self.game.physics_accumulator >= 0.05 {
            let mut input = CharacterInput {
                direction: if self.game.sectors_ready {
                    direction
                } else {
                    [0.0; 2]
                },
                jump: self.input.jump_pending && self.game.sectors_ready,
            };
            self.input.jump_pending = false;
            let previous_state = self.game.character;
            self.game.predict(input);
            if !self.refresh_active_sector_scene() {
                self.game.character = previous_state;
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
        self.poll_connector();
        self.poll_session();
        self.refresh_active_sector_scene();
        self.advance_pending_transitions();
        self.update_remote_player_rendering(Instant::now());

        let direction =
            camera_direction(self.game.character, self.input.yaw, self.input.pitch).normalize();
        let up = glam::Vec3::from_array(
            character_frame(self.game.character.gravity, self.game.character.position).0,
        );
        let camera_position = glam::Vec3::from_array(self.game.character.position) + up * 1.7;
        let view = glam::Mat4::look_to_rh(camera_position, direction, up);
        let projection = glam::Mat4::perspective_rh(
            70f32.to_radians(),
            self.gpu.size.width as f32 / self.gpu.size.height as f32,
            0.05,
            200.0,
        );
        self.gpu.queue.write_buffer(
            &self.gpu.camera_buffer,
            0,
            bytemuck::cast_slice(&[(projection * view).to_cols_array_2d()]),
        );

        let vertex_count = {
            let vertices = self.ui.console.build_vertices(
                self.gpu.size.width,
                self.gpu.size.height,
                self.game.health,
                self.input.cursor_locked && !self.ui.console.open,
                self.game
                    .hit_confirmation_until
                    .is_some_and(|until| Instant::now() < until),
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
    }
}

impl GameplayView {
    fn predict(&mut self, input: CharacterInput) {
        self.collision_query
            .fill(&self.collision_index, self.character, 0.05);
        step_character(
            &mut self.character,
            input,
            self.collision_query.candidates(),
            self.collision_query.surface_candidates(),
            0.05,
        );
    }
}

impl InputState {
    fn movement(&self, console_open: bool, character: CharacterState) -> [f32; 2] {
        let (up, base_right, base_forward) = character_frame(character.gravity, character.position);
        let base_right = glam::Vec3::from_array(base_right);
        let base_forward = glam::Vec3::from_array(base_forward);
        let _up = glam::Vec3::from_array(up);
        let forward = base_forward * self.yaw.cos() + base_right * self.yaw.sin();
        let right = base_right * self.yaw.cos() - base_forward * self.yaw.sin();
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
