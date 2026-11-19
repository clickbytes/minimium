use super::*;
use minimium_renderer::milky_way_gpu::{BolidGpuEvent, BolidGpuFrame, MAX_BOLIDS};
mod preparation;
pub(super) use preparation::BolidWorker;

impl DesktopHost {
    pub(super) fn bolid_notice(&mut self, notice: minimium_localization::UiNotice) {
        self.application.ui.console.push_ui_message(
            minimium_localization::UiMessage::Notice(notice),
            &self.localizer,
            self.application.settings.locale,
        );
    }

    pub(super) fn flush_pending_bolid(&mut self, now: Instant) {
        let Some(count) = self.pending_bolid_request else {
            return;
        };
        if self
            .application
            .connection_view()
            .session()
            .is_some_and(|session| session.state() == SessionState::Connected)
            && self
                .last_sky_view_at
                .is_some_and(|at| now.saturating_duration_since(at) >= Duration::from_millis(200))
        {
            self.pending_bolid_request = None;
            match self
                .application
                .request_random_bolid(count, crate::host_time::sample())
            {
                Ok(()) => self.bolid_notice(minimium_localization::UiNotice::BolidRequestSent),
                Err(error) => self
                    .application
                    .ui
                    .console
                    .push_line(format!("spawn_random_bolid: {error}")),
            }
        } else if self.application.connection_view().session().is_none() {
            self.pending_bolid_request = None;
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
            self.application.core_view().character,
            self.input.yaw,
            self.input.pitch,
        )
        .normalize()
        .to_array();
        let Some(calendar_epoch) = self.render_environment_frame.clock_revision else {
            return;
        };
        let Some(position_mm) = self.application.core_view().position_mm else {
            return;
        };
        // Allow 0.01 degree/s (over twice the diurnal angular speed) and a
        // 2 m position tolerance. Near twilight shorten the report to the margin.
        let radius = self
            .render_environment_frame
            .render_view
            .game_planet_radius_m;
        let margin =
            -6.0 - sky.sun.apparent_altitude_rad.to_degrees() - (2.0 / radius).to_degrees();
        let night_until = (margin > 0.0).then(|| {
            let span = (margin * 100_000.0).min(600_000.0) as i64;
            minimium_environment::time::WorldTimeMs::new(
                (sky.utc_ms.get() + span)
                    .min(minimium_environment::time::WorldTimeMs::MAX_EXCLUSIVE - 1),
            )
            .expect("bounded solar interval")
        });
        let observation = minimium_protocol::SkyObservation {
            position_mm,
            calendar_epoch,
            utc: sky.utc_ms,
            night_until,
        };
        if self
            .application
            .report_sky_view(context, direction, observation, crate::host_time::sample())
            .unwrap_or(false)
        {
            self.last_sky_view_at = Some(now);
        }
    }

    pub(super) fn prepare_bolids(&mut self) -> BolidGpuFrame {
        let mut frame = BolidGpuFrame::default();
        let context = self.render_environment_frame.context;
        let length_scale = self
            .application
            .core_view()
            .manifest
            .and_then(|m| m.planet())
            .filter(|p| p.mode == 1)
            .map_or(1.0, |p| f64::from(p.height_scale_permille) / 1000.0);
        let radius = self
            .render_environment_frame
            .render_view
            .game_planet_radius_m;
        while let Some((key, model)) = self.bolid_worker.poll() {
            // A late worker result cannot resurrect a removed/transferred event
            // or overwrite a diagnostic replay with a different path or seed.
            if Some(key.context) == context && key.radius == radius && key.scale == length_scale {
                if let Some((_, _, cached)) = self
                    .bolids
                    .iter_mut()
                    .find(|(c, e, _)| *c == key.context && *e == key.event)
                {
                    *cached = Some(model);
                }
            }
        }
        let server_now = self.environment_frame.simulation_ms;
        for (event_context, event, cached) in &mut self.bolids {
            if Some(*event_context) != context || frame.count[0] as usize >= MAX_BOLIDS {
                continue;
            }
            let live_age = (server_now.unwrap_or(event.start_simulation_ms) as i128
                - event.start_simulation_ms as i128) as f32
                / 1000.0;
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
            if age < 0.0 || age > 60.0 + lifetime {
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
            let Some(evolution) = cached.as_ref() else {
                self.bolid_worker.request(
                    preparation::Key {
                        context: *event_context,
                        event: *event,
                        radius,
                        scale: length_scale,
                    },
                    frame.events[frame.count[0] as usize],
                );
                continue;
            };
            if age > evolution.luminous_end() + lifetime {
                continue;
            }
            evolution.sample(
                &mut frame.events[frame.count[0] as usize],
                self.render_environment_frame.render_view.render_origin_m,
            );
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
