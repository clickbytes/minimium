use super::*;
use crate::messages::ClientText as Text;

impl Renderer {
    pub(super) fn update(&mut self) -> environment::PreparedEnvironmentFrame {
        if self.overview_open && self.overview_instance != Some(self.game.current_instance) {
            self.overview_open = false;
            self.gpu.overview_active = false;
        }
        let now = Instant::now();
        let dt = now
            .duration_since(self.metrics.last_frame)
            .as_secs_f32()
            .min(minimium_core::world::SIMULATION_STEP_SECONDS);
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
        self.game.accumulate_prediction_time(dt);
        let manifest = self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .copied();
        while let Some(predicted) = self.game.predict_next(
            direction,
            self.input.jump_pending,
            self.game.sectors_ready && !self.overview_open,
            manifest,
        ) {
            let mut input = predicted.input;
            self.input.jump_pending = false;
            if !self.refresh_active_sector_scene() {
                self.game.character = predicted.previous_state;
                self.game.position_mm = predicted.previous_mm;
                input = CharacterInput::default();
            }
            self.input.input_keepalive += minimium_core::world::SIMULATION_STEP_SECONDS;
            let moving = input.direction != [0.0, 0.0];
            let input_changed = input.direction != self.input.last_sent_direction;
            if self
                .connection
                .session()
                .is_some_and(|session| session.state() == SessionState::Connected)
                && (input.jump
                    || input_changed
                    || (moving
                        && self.input.input_keepalive
                            >= 2.0 * minimium_core::world::SIMULATION_STEP_SECONDS))
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
        }
        let origin = if self
            .game
            .world_manifests
            .get(&self.game.current_instance)
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
            0.05
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
        let mut prepared_environment = match render_view {
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

        let earth_location = prepared_environment
            .frame
            .descriptor
            .filter(|descriptor| {
                descriptor.profile() == minimium_core::environment::EnvironmentProfile::Earth
            })
            .and(prepared_environment.frame.observer)
            .map(|observer| {
                let geographic = observer.geographic;
                (
                    geographic.latitude_rad().to_degrees(),
                    geographic.longitude_rad().to_degrees(),
                )
            });
        let world_utc_ms = prepared_environment
            .frame
            .sky
            .as_ref()
            .map(|sky| sky.utc_ms.get());
        let seed = manifest.map_or(0, |manifest| manifest.generation().seed().get());
        self.weather.update(
            earth_location,
            world_utc_ms,
            seed,
            self.environment_frame.rate_ppm,
            self.environment_frame.revision,
            manifest
                .and_then(|world| world.planet())
                .filter(|planet| planet.mode == 1)
                .map(|planet| crate::environment::weather::TerrainConfig {
                    height_scale_permille: planet.height_scale_permille,
                    sea_level_mm: planet.sea_level_mm,
                    radius_m: game_planet_radius_m,
                }),
        );
        let (atlas, revision, blend, region, atlas_start) = self.weather.atlas();
        self.gpu
            .weather_gpu
            .stage_atlas(atlas, revision, region, atlas_start);
        let settings = self.gpu.weather_gpu.cloud_settings.effective(
            self.sky_quality.effective == crate::environment::quality::SkyQuality::Standard,
            &self.gpu.device.limits(),
            self.gpu.weather_gpu.capabilities.precise,
        );
        if settings.scale != self.gpu.weather_gpu.cloud_effective.scale {
            self.gpu.weather_gpu.cloud_effective = settings;
            self.gpu
                .weather_gpu
                .resize(&self.gpu.device, self.gpu.size, &self.gpu.depth_view);
        }
        self.gpu.weather_gpu.cloud_effective = settings;
        self.gpu.weather_gpu.instances.update(
            &self.gpu.queue,
            cloud_instances::View {
                clock_revision: self.environment_frame.revision,
                world_rate: self.environment_frame.rate_ppm.unwrap_or(1_000_000) as f64
                    / 1_000_000.0,
                eye: camera_eye_world,
                radius: if earth_location.is_some() {
                    game_planet_radius_m
                } else {
                    0.0
                },
                pixels_per_radian: f64::from(self.gpu.size.height)
                    / (2.0 * f64::from((self.fov_degrees.to_radians() * 0.5).tan())),
                utc_ms: world_utc_ms.unwrap_or(0),
                blend,
                seed,
                settings,
            },
        );
        self.gpu.weather_gpu.instances.upload_view(
            &self.gpu.queue,
            camera_eye_world,
            settings,
            [self.gpu.size.width, self.gpu.size.height],
        );
        self.gpu.weather_gpu.publish_atlas(&self.gpu.queue);
        let presented = earth_location.and_then(|location| {
            self.gpu
                .weather_gpu
                .instances
                .presentation(location, world_utc_ms.unwrap_or(0))
        });
        let (weather, blend, region) = if let Some(frame) = &presented {
            (frame.sample, frame.blend, frame.region)
        } else {
            (
                crate::environment::weather::WeatherSample::clear(),
                0.0,
                [0.0; 4],
            )
        };
        let rain_ceiling_m = presented.as_ref().map_or(0.0, |frame| frame.rain_ceiling_m);
        self.weather
            .present(weather, presented.and_then(|frame| frame.lightning));
        if earth_location.is_some() {
            prepared_environment.apply_cloud_light([weather.low, weather.middle, weather.high]);
            if !self.overview_open {
                prepared_environment.apply_weather_fog(
                    weather.fog,
                    weather.ground_moisture,
                    weather.temperature_c <= 0.0,
                );
            }
        }
        if let Some(flash) = self.weather.lightning() {
            prepared_environment.apply_lightning_light(flash.intensity);
        }
        if let Some(notice) = self.weather.take_notice() {
            self.ui.console.push_line(notice);
        }
        let shelter_mask = if earth_location.is_some() && !self.overview_open {
            let eye = prepared_environment
                .frame
                .render_view
                .camera_eye_world_m
                .map(|value| value as f32);
            let up = glam::Vec3::from_array(eye).normalize_or_zero().to_array();
            self.weather
                .schedule_thunder(eye, game_planet_radius_m as f32);
            if weather.rain_mm_h > 0.05 {
                self.weather
                    .shelter_mask(&self.game.collision_index, eye, up)
            } else {
                [0; 4]
            }
        } else {
            [0; 4]
        };
        self.gpu.weather_gpu.upload(
            &self.gpu.queue,
            weather_gpu::WeatherFrame {
                rain_ceiling_m,
                readable_globe: self.weather.readable_globe,
                sample: weather,
                region,
                overview: self.overview_open,
                seed,
                blend,
                radius: game_planet_radius_m as f32,
                origin: if self.overview_open {
                    [0.0; 3]
                } else {
                    self.gpu.render_origin
                },
                utc_ms: world_utc_ms,
                lightning: self.weather.lightning(),
                standard_quality: self.sky_quality.effective
                    == crate::environment::quality::SkyQuality::Standard,
                shelter_mask,
            },
        );

        let celestial_labels = if self.show_celestial_names {
            celestial_labels::prepare(&prepared_environment.frame, self.sky_catalogue.as_ref())
        } else {
            Vec::new()
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
                    celestial_labels: &celestial_labels,
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
    minimium_core::world::GravityPolicy::for_manifest(manifest).character_gravity()
}
