//! Presentation of server-owned bodies. No local terrain or physics authority.
use minimium_protocol::WorldContext;
use minimium_renderer::meteors::{
    model::{MeteorPhase, MeteorSample, MeteorSource, MeteorTrailSample},
    trail::TrailKind,
    MeteorFrame,
};
use minimium_renderer::transient_lighting::TransientLightSample;
use minimium_world::{
    impact::BodyState,
    terrain_edits::{length, project},
};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    time::Instant,
};

struct Track {
    body: BodyState,
    received: Instant,
    history: VecDeque<[f64; 3]>,
}
struct Burst {
    body: BodyState,
    at: Instant,
    sound: bool,
    started: bool,
}
#[derive(Default)]
pub(super) struct TrackedImpacts {
    pub(super) published_terrain: Option<(WorldContext, minimium_procgen::SectorContentVersion)>,
    pub(super) published_craters: HashSet<u64>,
    context: Option<WorldContext>,
    tracks: BTreeMap<u64, Track>,
    bursts: VecDeque<Burst>,
}

impl TrackedImpacts {
    pub(super) fn observe(&mut self, context: WorldContext, body: BodyState, live: bool) {
        if self.context != Some(context) {
            *self = Self {
                context: Some(context),
                ..Self::default()
            };
        }
        if body.phase == 0 {
            if self.tracks.len() >= 8 && !self.tracks.contains_key(&body.id) {
                return;
            }
            let track = self.tracks.entry(body.id).or_insert_with(|| Track {
                body,
                received: Instant::now(),
                history: VecDeque::with_capacity(8),
            });
            if track.history.len() == 8 {
                track.history.pop_front();
            }
            track.history.push_back(body.position);
            track.body = body;
            track.received = Instant::now();
        } else {
            self.tracks.remove(&body.id);
            if live && body.phase == 1 && !self.bursts.iter().any(|b| b.body.id == body.id) {
                if self.bursts.len() == 8 {
                    self.bursts.pop_front();
                }
                self.bursts.push_back(Burst {
                    body,
                    at: Instant::now(),
                    sound: false,
                    started: false,
                });
            }
        }
    }

    pub(super) fn append(
        &mut self,
        context: Option<WorldContext>,
        frame: &mut MeteorFrame,
        now_s: f64,
        audio: &mut crate::audio::MeteorAudio,
    ) {
        if context != self.context {
            *self = Self {
                context,
                ..Self::default()
            };
            return;
        }
        self.tracks
            .retain(|_, t| t.received.elapsed().as_secs_f64() < 10.0);
        self.bursts.retain(|b| b.at.elapsed().as_secs_f64() < 12.0);
        for track in self.tracks.values() {
            let body = track.body;
            let dt = track.received.elapsed().as_secs_f64().min(0.08);
            let position = std::array::from_fn(|i| {
                body.position[i] + body.velocity[i] * body.metres_scale * dt
            });
            let speed = length(body.velocity);
            let altitude = ((length(position) - body.planet_radius_m) / body.metres_scale).max(0.0);
            let density = 1.225 * (-altitude / 7800.0).exp();
            let flicker = 0.85 + 0.15 * (body.age_s * 31.0 + (body.seed % 1000) as f64).sin();
            let power = (0.01
                * density
                * std::f64::consts::PI
                * body.radius_m().powi(2)
                * speed.powi(3)
                * flicker)
                .min(1e11);
            if speed < 2500.0 || power < 1.0 {
                continue;
            }
            let rgb = match body.seed % 4 {
                0 => [0.72, 1.0, 0.8],
                1 => [0.7, 0.85, 1.0],
                2 => [1.0, 0.82, 0.5],
                _ => [1.0, 0.95, 0.87],
            };
            let id = (1_u64 << 60) | body.id * 16;
            core(
                frame,
                id,
                position,
                rgb,
                power,
                body.radius_m().max(0.3),
                now_s,
            );
            for (serial, p) in track.history.iter().enumerate() {
                trail(
                    frame,
                    id,
                    serial as u32,
                    *p,
                    0.7,
                    0.35 + serial as f64 * 0.06,
                    now_s,
                    TrailKind::Wake,
                );
            }
            light(frame, id, position, rgb, power / 12.566, 15000.0);
        }
        for burst in &mut self.bursts {
            if !burst.started {
                if !self
                    .published_terrain
                    .is_some_and(|(c, _)| Some(c) == context)
                    || !self.published_craters.contains(&burst.body.id)
                {
                    continue;
                }
                burst.started = true;
                burst.at = Instant::now();
            }
            let body = burst.body;
            let age = burst.at.elapsed().as_secs_f64();
            let up = project(body.position, 1.0);
            let center = std::array::from_fn(|i| body.position[i] + up[i] * 3.0);
            let id = (1_u64 << 60) | body.id * 16;
            let power = (body.energy_j * 0.015).min(1e11) * (-age / 0.16).exp();
            core(
                frame,
                id,
                center,
                [1.0, 0.87, 0.68],
                power.max(0.001),
                if age < 0.8 { 1.0 + age * 12.0 } else { 0.0 },
                now_s,
            );
            if age < 1.2 {
                light(frame, id, center, [1.0, 0.87, 0.68], power / 12.566, 3000.0);
            }
            let axis = if up[1].abs() < 0.9 {
                [0.0, 1.0, 0.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            let east = project(
                [
                    up[1] * axis[2] - up[2] * axis[1],
                    up[2] * axis[0] - up[0] * axis[2],
                    up[0] * axis[1] - up[1] * axis[0],
                ],
                1.0,
            );
            let north = [
                up[1] * east[2] - up[2] * east[1],
                up[2] * east[0] - up[0] * east[2],
                up[0] * east[1] - up[1] * east[0],
            ];
            // Dust is an extinction layer, not an emissive glowing cloud.
            let dust = id + 1;
            core(frame, dust, center, [0.55, 0.47, 0.38], 1.0, 0.0, now_s);
            for serial in 0..8 {
                let angle = serial as f64 / 7.0 * std::f64::consts::TAU;
                let spread = body.crater_radius_m * (0.3 + age * 0.13);
                let p = std::array::from_fn(|i| {
                    center[i]
                        + up[i] * age * 2.0
                        + (east[i] * angle.cos() + north[i] * angle.sin()) * spread
                });
                trail(
                    frame,
                    dust,
                    serial,
                    p,
                    2.0 + age * 0.8,
                    0.45 * (1.0 - age / 12.0).powi(2),
                    now_s,
                    TrailKind::Dust,
                );
            }
            for fragment in 0..3 {
                let angle = fragment as f64 * 2.1 + (body.seed % 100) as f64;
                let elevation = 12.0 * age - 4.905 * age * age;
                if elevation < 0.0 {
                    continue;
                }
                let p = std::array::from_fn(|i| {
                    center[i]
                        + up[i] * elevation
                        + (east[i] * angle.cos() + north[i] * angle.sin()) * age * 16.0
                });
                core(
                    frame,
                    id + 2 + fragment,
                    p,
                    [1.0, 0.43, 0.13],
                    30000.0 * (-age / 0.4).exp(),
                    0.12,
                    now_s,
                );
            }
            if !burst.sound {
                burst.sound = true;
                audio.enqueue(
                    crate::audio::meteors::MeteorSoundCue {
                        source_pos: physical(frame, center),
                        onset_monotonic: now_s - age,
                        relative_energy: (body.energy_j / 1e11).clamp(0.1, 1.0) as f32,
                        cue_kind: crate::audio::meteors::CueKind::Airburst,
                        seed: body.seed,
                    },
                    physical(frame, frame.context().camera_eye_world_m),
                    east,
                    now_s,
                );
            }
        }
    }
}

fn physical(frame: &MeteorFrame, p: [f64; 3]) -> [f64; 3] {
    let c = frame.context();
    project(
        p,
        c.game_radius_m
            + c.surface_scale
                .physical_altitude_m(length(p) - c.game_radius_m),
    )
}
fn core(
    frame: &mut MeteorFrame,
    id: u64,
    p: [f64; 3],
    rgb: [f64; 3],
    power: f64,
    radius: f64,
    time: f64,
) {
    if let Ok(sample) = MeteorSample::new(
        id,
        MeteorSource::Tracked,
        time,
        physical(frame, p),
        [0.0; 3],
        rgb,
        power,
        radius,
        MeteorPhase::Peak,
    ) {
        frame.push_core(sample);
    }
}
fn trail(
    frame: &mut MeteorFrame,
    id: u64,
    serial: u32,
    p: [f64; 3],
    width: f64,
    opacity: f64,
    time: f64,
    kind: TrailKind,
) {
    if let Ok(mut sample) =
        MeteorTrailSample::new(id, serial, time, physical(frame, p), width, opacity)
    {
        sample.kind = kind;
        frame.push_trail(sample);
    }
}
fn light(frame: &mut MeteorFrame, id: u64, p: [f64; 3], rgb: [f64; 3], power: f64, range: f64) {
    if let Ok(sample) =
        TransientLightSample::new(id, physical(frame, p), rgb, power, range, 0.1, 1.0)
    {
        frame.push_light(sample);
    }
}
