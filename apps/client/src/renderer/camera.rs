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
        let direction = self.input.movement(self.ui.console.open);
        self.game.physics_accumulator += dt;
        while self.game.physics_accumulator >= 0.05 {
            let input = CharacterInput {
                direction: [direction.x, direction.z],
                jump: self.input.jump_pending,
            };
            self.input.jump_pending = false;
            self.game.predict(input);
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
        self.update_remote_player_rendering(Instant::now());

        let direction = self.game.look_direction(self.input.yaw, self.input.pitch);
        let up = self.game.up_direction();
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
    fn up_direction(&self) -> glam::Vec3 {
        self.planet
            .filter(|sphere| sphere.contains(self.character.position))
            .map(|sphere| glam::Vec3::from_array(sphere.up_at(self.character.position)))
            .unwrap_or(glam::Vec3::Y)
    }

    pub(super) fn look_direction(&self, yaw: f32, pitch: f32) -> glam::Vec3 {
        let local = glam::Vec3::new(
            yaw.cos() * pitch.cos(),
            pitch.sin(),
            yaw.sin() * pitch.cos(),
        );
        if let Some(sphere) = self.planet.filter(|sphere| sphere.contains(self.character.position)) {
            let (east, up, north) = sphere.frame_at(self.character.position);
            (glam::Vec3::from_array(east) * local.x
                + glam::Vec3::from_array(up) * local.y
                + glam::Vec3::from_array(north) * local.z)
                .normalize()
        } else {
            local.normalize()
        }
    }

    fn predict(&mut self, input: CharacterInput) {
        if let Some(sphere) = self.planet.filter(|sphere| sphere.contains(self.character.position)) {
            step_character_on_planet(&mut self.character, input, sphere, 0.05);
        } else {
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
        if let Some(destination) = portal_destination(self.character.position, &self.portals) {
            self.character.position = destination;
            self.character.velocity = [0.0; 3];
            self.character.grounded = false;
        }
    }
}

impl InputState {
    fn movement(&self, console_open: bool) -> glam::Vec3 {
        let forward = glam::Vec3::new(self.yaw.cos(), 0.0, self.yaw.sin());
        let right = glam::Vec3::new(-forward.z, 0.0, forward.x);
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
            movement.normalize()
        } else {
            movement
        }
    }
}
