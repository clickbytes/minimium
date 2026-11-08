use super::*;
use minimium_renderer::milky_way_gpu::{BolidGpuEvent, BolidGpuFrame, MAX_BOLIDS};

impl Renderer {
    pub(super) fn bolid_notice(&mut self, notice: minimium_localization::UiNotice) {
        self.application.ui.console.push_ui_message(
            minimium_localization::UiMessage::Notice(notice),
            &self.localizer,
            self.application.settings.locale,
        );
    }

    pub(super) fn flush_pending_bolid(&mut self, now: Instant) {
        if !self.pending_bolid_request {
            return;
        }
        if self
            .application
            .connection
            .session()
            .is_some_and(|session| session.state() == SessionState::Connected)
            && self
                .last_sky_view_at
                .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_millis(200))
        {
            self.pending_bolid_request = false;
            match self.application.connection.request_random_bolid() {
                Ok(()) => self.bolid_notice(minimium_localization::UiNotice::BolidRequestSent),
                Err(error) => self
                    .application
                    .ui
                    .console
                    .push_line(format!("spawn_random_bolid: {error}")),
            }
        } else if self.application.connection.session().is_none() {
            self.pending_bolid_request = false;
            self.application
                .ui
                .console
                .push_line("spawn_random_bolid: local server connection ended");
        }
    }

    pub(super) fn send_sky_view(&mut self, now: Instant) {
        if self
            .last_sky_view_at
            .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_secs(1))
        {
            return;
        }
        let Some(context) = self.render_environment_frame.context else {
            return;
        };
        let Some(sky) = self.render_environment_frame.sky.as_ref() else {
            return;
        };
        if self.application.overview().is_open() {
            return;
        }
        let direction = camera::camera_direction(
            self.application.game.gameplay.prediction.character,
            self.input.yaw,
            self.input.pitch,
        )
        .normalize()
        .to_array();
        let night = sky.sun.apparent_altitude_rad < (-6.0_f64).to_radians();
        if let Some(session) = self.application.connection.session_mut() {
            if session
                .queue(&minimium_protocol::ClientMessage::SkyView {
                    context,
                    direction,
                    night,
                })
                .is_ok()
            {
                self.last_sky_view_at = Some(now);
            }
        }
    }

    pub(super) fn prepare_bolids(&mut self) -> BolidGpuFrame {
        let mut frame = BolidGpuFrame::default();
        let context = self.render_environment_frame.context;
        let length_scale = self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
            .and_then(|m| m.planet())
            .filter(|p| p.mode == 1)
            .map_or(1.0, |p| f64::from(p.height_scale_permille) / 1000.0);
        let server_now = self
            .application
            .connection
            .session()
            .and_then(|session| session.server_monotonic_ms());
        for (event_context, event, received_at, cached) in &mut self.bolids {
            if Some(*event_context) != context || frame.count[0] as usize >= MAX_BOLIDS {
                continue;
            }
            let live_age = server_now.map_or_else(
                || received_at.elapsed().as_secs_f32(),
                |now| (now as i128 - event.start_monotonic_ms as i128) as f32 / 1000.0,
            );
            let age = self
                .diagnostic_bolid_age
                .filter(|(c, id, _)| c == event_context && *id == event.id)
                .map_or(live_age, |(_, _, age)| age);
            let duration = event.duration_ms as f32 / 1000.0;
            let persistent = (event.kind == 2 && event.seed % 3 == 0)
                || (event.kind == 1 && event.seed % 19 == 0);
            let lifetime = if persistent {
                if event.seed % 31 == 0 {
                    90.0
                } else {
                    20.0 + (event.seed % 41) as f32
                }
            } else {
                3.5
            };
            if age < -0.25 || age > 60.0 + lifetime {
                continue;
            }
            frame.events[frame.count[0] as usize] = BolidGpuEvent {
                start_age: [
                    event.start_world_m[0],
                    event.start_world_m[1],
                    event.start_world_m[2],
                    age,
                ],
                end_duration: [
                    event.end_world_m[0],
                    event.end_world_m[1],
                    event.end_world_m[2],
                    duration,
                ],
                style: [
                    event.kind as f32,
                    ((event.seed >> 16) as u16 as f32) / u16::MAX as f32,
                    lifetime,
                    if event.kind == 2 && event.seed % 9 == 0 {
                        1.0
                    } else {
                        0.0
                    },
                ],
                ..Default::default()
            };
            let evolution = cached.get_or_insert_with(|| {
                minimium_renderer::milky_way_gpu::BolidEvolution::new(
                    &frame.events[frame.count[0] as usize],
                    self.render_environment_frame
                        .render_view
                        .game_planet_radius_m,
                    length_scale,
                )
            });
            if age > evolution.luminous_end() + lifetime {
                continue;
            }
            evolution.sample(&mut frame.events[frame.count[0] as usize]);
            frame.count[0] += 1;
        }
        frame
    }
}

pub(super) fn strongest_bolid_light(
    frame: &BolidGpuFrame,
    eye: [f64; 3],
    atmosphere: &minimium_renderer::AtmosphereUniform,
) -> Option<([f32; 4], [f32; 4])> {
    frame
        .events
        .iter()
        .take(frame.count[0] as usize)
        .map(|e| {
            let color = [e.emission[0], e.emission[1], e.emission[2], 1.0];
            let strength = minimium_renderer::bolid_light::incident_luminance(
                e.light_position_power,
                color,
                eye,
                atmosphere,
            );
            (e.light_position_power, color, strength)
        })
        .filter(|(_, _, strength)| *strength > 0.0)
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(position, color, _)| (position, color))
}
