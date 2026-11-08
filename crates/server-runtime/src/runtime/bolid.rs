mod framing;
use super::ServerRuntime;
use crate::{commands::CommandReply, connection::queue_message};
use minimium_protocol::{BolidEvent, ServerMessage};
use std::time::{Duration, Instant};

impl ServerRuntime {
    pub(super) fn schedule_next_bolid(&mut self, now: Instant) {
        self.next_bolid_at = self.bolid_interval.map(|(minimum, maximum)| {
            let span = maximum - minimum + 1;
            let jitter = mix(self.next_bolid_id ^ self.scheduler.simulation_tick) % span;
            now + Duration::from_secs(minimum + jitter)
        });
    }

    pub(super) fn update_bolids(&mut self) {
        let now = Instant::now();
        if self.next_bolid_at.is_some_and(|deadline| now >= deadline) {
            let _ = self.spawn_bolid(false);
            self.schedule_next_bolid(now);
        }
        self.sky_views.retain(|player, (context, _, _, at)| {
            now.saturating_duration_since(*at) < Duration::from_secs(15)
                && self.game.worlds.context_matches(*player, *context)
        });
    }

    pub(super) fn spawn_bolid(&mut self, manual: bool) -> CommandReply {
        let fail = |text: &str, code| CommandReply {
            success: false,
            text: text.to_owned(),
            stop: false,
            error_code: Some(code),
        };
        let now = Instant::now();
        if manual
            && self
                .last_manual_bolid_at
                .is_some_and(|at| now.saturating_duration_since(at) < Duration::from_secs(5))
        {
            return fail(
                "bolid request rate limited; try again in five seconds",
                "server.bolid.rate_limited",
            );
        }
        let players = self.game.worlds.player_snapshots();
        let candidates: Vec<_> = players
            .iter()
            .filter_map(|(instance, player)| {
                let (context, gaze, night, at) = *self.sky_views.get(&player.id)?;
                let owner = self.game.worlds.iter().find(|(id, _)| id == instance)?.1;
                if !self.game.worlds.context_matches(player.id, context)
                    || owner.manifest.planet().is_none()
                    || (!manual && !night)
                    || now.saturating_duration_since(at) > Duration::from_secs(15)
                {
                    return None;
                }
                Some((
                    *instance,
                    player.id,
                    player.position,
                    gaze,
                    owner.manifest.planet()?.radius_mm as f32 / 1000.0,
                    if owner.manifest.planet()?.mode == 1 {
                        owner.manifest.planet()?.height_scale_permille as f32 / 1000.0
                    } else {
                        1.0
                    },
                ))
            })
            .collect();
        let Some(&(instance, player_id, position, gaze, radius, scale_m)) =
            candidates.get((mix(self.next_bolid_id) as usize) % candidates.len().max(1))
        else {
            return fail(
                "no active planet player with a recent sky view",
                "server.bolid.no_observer",
            );
        };
        let up = normalize(position);
        let outside = length_squared(position).sqrt() > radius + 110_000.0 * scale_m;
        let direction = if !outside && dot(gaze, up) < 0.12 {
            normalize(add(gaze, scale(up, 0.35 - dot(gaze, up))))
        } else {
            gaze
        };
        let seed = mix(self.next_bolid_id ^ self.scheduler.simulation_tick ^ player_id.get());
        let kind = if manual {
            // Manual requests are a bright specimen for visual inspection.
            // The seed still chooses trajectory, material and breakup family.
            2
        } else if seed % 100 == 0 {
            2
        } else if seed % 10 == 0 {
            1
        } else {
            0
        };
        let (start, end) = if manual {
            let Some(path) = framing::in_view(position, gaze, radius, scale_m, seed, kind) else {
                return fail(
                    "look toward a wider visible part of the atmosphere to place the whole bolid",
                    "server.bolid.no_atmosphere_in_view",
                );
            };
            path
        } else {
            let Some(distance) =
                atmospheric_intersection(position, direction, radius + 76_000.0 * scale_m)
            else {
                return fail(
                    "look toward the atmosphere to place a bolid in view",
                    "server.bolid.no_atmosphere_in_view",
                );
            };
            let center = add(position, scale(direction, distance));
            let radial = normalize(center);
            let axis = if radial[2].abs() < 0.9 {
                [0.0, 0.0, 1.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            let tangent = normalize(cross(radial, axis));
            let cross_tangent = cross(radial, tangent);
            let angle = (seed as u32 as f32 / u32::MAX as f32) * std::f32::consts::TAU;
            let side = add(
                scale(tangent, angle.cos()),
                scale(cross_tangent, angle.sin()),
            );
            let lateral = [38_000.0, 85_000.0, 145_000.0][kind as usize]
                * (0.8 + ((seed >> 32) as u32 as f32 / u32::MAX as f32) * 0.4)
                * scale_m;
            let start_height = [92_000.0, 101_000.0, 108_000.0][kind as usize];
            let end_height = [61_000.0, 48_000.0, 37_000.0][kind as usize];
            let start = scale(
                normalize(add(center, scale(side, -lateral * 0.5))),
                radius + start_height * scale_m,
            );
            let end = scale(
                normalize(add(center, scale(side, lateral * 0.5))),
                radius + end_height * scale_m,
            );
            (start, end)
        };
        let path_length = length_squared(sub(end, start)).sqrt();
        let speed = [22_000.0, 35_000.0, 51_000.0][kind as usize]
            * (0.8 + ((seed >> 48) as u16 as f32 / u16::MAX as f32) * 0.4);
        let start_monotonic_ms = self.monotonic_ms(now).map_or(0, |time| time.get());
        let event = BolidEvent {
            id: self.next_bolid_id,
            start_monotonic_ms,
            seed,
            start_world_m: start,
            end_world_m: end,
            duration_ms: (path_length / (speed * scale_m) * 1000.0).round() as u16,
            kind,
        };
        self.next_bolid_id = self.next_bolid_id.wrapping_add(1).max(1);
        let mut recipients = 0;
        for client in &mut self.connections.clients {
            let Some(id) = client.player_id() else {
                continue;
            };
            let Some(context) = self.game.worlds.context_for_player(id) else {
                continue;
            };
            if context.instance() != instance {
                continue;
            }
            let Some(peer) = self.game.worlds.player_position_in_context(id, context) else {
                continue;
            };
            if length_squared(sub(peer, position)) > 250_000.0_f32.powi(2) {
                continue;
            }
            if queue_message(
                client,
                &mut self.metrics,
                ServerMessage::Bolid { context, event },
            ) {
                recipients += 1;
            }
        }
        if manual {
            self.last_manual_bolid_at = Some(now);
        }
        CommandReply {
            success: recipients > 0,
            text: format!(
                "bolid id={} kind={} recipients={recipients}",
                event.id, event.kind
            ),
            stop: false,
            error_code: (recipients == 0).then_some("server.bolid.delivery_failed"),
        }
    }
}

// Observers above the luminous layer need the near intersection. Ground
// observers need the exit intersection; neither distance is clamped to 350 km.
fn atmospheric_intersection(position: [f32; 3], direction: [f32; 3], shell: f32) -> Option<f32> {
    let b = (0..3)
        .map(|i| f64::from(position[i]) * f64::from(direction[i]))
        .sum::<f64>();
    let r2 = position.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>();
    let c = r2 - f64::from(shell).powi(2);
    let d = b * b - c;
    if d < 0.0 {
        return None;
    }
    let near = -b - d.sqrt();
    let far = -b + d.sqrt();
    let distance = if near > 0.0 { near } else { far };
    (distance > 0.0 && distance.is_finite()).then_some(distance as f32)
}

fn mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58476d1ce4e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d049bb133111eb);
    value ^ (value >> 31)
}
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn scale(a: [f32; 3], factor: f32) -> [f32; 3] {
    [a[0] * factor, a[1] * factor, a[2] * factor]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn length_squared(a: [f32; 3]) -> f32 {
    dot(a, a)
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    scale(a, length_squared(a).sqrt().max(0.0001).recip())
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
