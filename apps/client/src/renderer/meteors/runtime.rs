//! Manual meteor lifecycle mechanics: frame building, spawn/clear and status.

use super::{
    ActiveEvent, ContextState, EventBasis, MeteorRuntime, TrailSlot, TRAIL_SLOTS_PER_EVENT,
};
use crate::client_commands::MeteorPreset;
use crate::renderer::environment::{EnvironmentFrame, EnvironmentFrameStatus, RenderViewFrame};
use minimium_renderer::meteors::{
    coords::PlanetSurfaceScale,
    model::{
        MeteorPhase, MeteorSample, MeteorSource, MeteorTrailSample, TransientLightSample,
        POSITION_ABS_M_MAX,
    },
    MeteorDisabledReason, MeteorFrame, MeteorFrameContext, MeteorFrameState,
};
use std::f64::consts::TAU;

impl MeteorRuntime {
    /// Creates the runtime with its single monotonic datum.
    pub(in crate::renderer) fn new() -> Self {
        Self {
            epoch: std::time::Instant::now(),
            next_id: 1,
            active: Vec::new(),
            spawned_total: 0,
            dismissed_total: 0,
            last_seed: 0,
            context: None,
            context_state: ContextState::Pending,
        }
    }

    /// Client-monotonic runtime time in seconds (ADR 0013 § 5).
    fn now_s(&self) -> f64 {
        self.epoch.elapsed().as_secs_f64()
    }

    /// Spawns one deterministic manual event. Accepted only while the last
    /// built frame admitted meteors; the event initializes from the next valid
    /// environment frame so a spawn that races a teleport never memorizes a
    /// stale view.
    pub(in crate::renderer) fn spawn(&mut self, preset: MeteorPreset) -> Result<String, String> {
        if self.active.len() >= super::MAX_ACTIVE_METEOR_EVENTS {
            return Err(
                "meteors: active limit (8) reached; run sky meteors clear first".to_owned(),
            );
        }
        match &self.context_state {
            ContextState::Available { .. } => {}
            ContextState::Pending => {
                return Err("meteors: no environment context yet; event not spawned".to_owned());
            }
            ContextState::Unavailable { reason } => {
                return Err(format!(
                    "meteors: environment unavailable ({reason}); event not spawned"
                ));
            }
        }
        let now_s = self.now_s();
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let seed = self.derive_seed(now_s);
        self.active.push(ActiveEvent::new(id, preset, seed, now_s));
        self.spawned_total = self.spawned_total.wrapping_add(1);
        self.last_seed = seed;
        Ok(format!(
            "meteors: spawned {} meteor {id} (seed 0x{seed:016x}); initializes on the next valid environment frame",
            preset.label()
        ))
    }

    /// Clears every active event; identical input sets stay deterministic.
    pub(in crate::renderer) fn clear(&mut self) -> usize {
        let cleared = self.active.len();
        self.reset_active();
        cleared
    }

    /// Rebuilds the manual meteor frame once per client frame from the freshly
    /// prepared environment frame. Disabled states, missing context, world
    /// scope changes, reconnects and teleports dismiss pending events
    /// instantly (ADR 0013 § 5); the origin rebase happens inside the frame.
    pub(in crate::renderer) fn build_frame(&mut self, frame: &EnvironmentFrame) -> MeteorFrame {
        match frame.status {
            EnvironmentFrameStatus::Available => {}
            EnvironmentFrameStatus::Interior => {
                self.reset_active();
                self.context = None;
                self.context_state = ContextState::Unavailable {
                    reason: "interior profile forbids meteors",
                };
                return MeteorFrame::disabled(MeteorDisabledReason::Interior);
            }
            EnvironmentFrameStatus::Unavailable => {
                self.reset_active();
                self.context = None;
                self.context_state = ContextState::Unavailable {
                    reason: "environment unavailable",
                };
                return MeteorFrame::disabled(MeteorDisabledReason::NoEnvironment);
            }
        }
        let Some(context) = frame.context else {
            self.reset_active();
            self.context = None;
            self.context_state = ContextState::Unavailable {
                reason: "environment frame lacks a world context",
            };
            return MeteorFrame::disabled(MeteorDisabledReason::NoEnvironment);
        };
        // Same instance with a new content revision, a teleport or a new
        // membership epoch: the whole frame set re-resolves, so pending manual
        // events drop instead of living across stale world data.
        if self.context.is_some_and(|previous| previous != context) {
            self.reset_active();
        }
        self.context = Some(context);
        let view = frame.render_view;
        let scale = frame.meteor_surface_scale;
        let mut meteor_frame = MeteorFrame::ready(MeteorFrameContext {
            camera_eye_world_m: view.camera_eye_world_m,
            render_origin_m: view.render_origin_m,
            game_radius_m: view.game_planet_radius_m,
            surface_scale: scale,
        });
        if meteor_frame.state() != MeteorFrameState::Ready {
            self.reset_active();
            self.context_state = ContextState::Unavailable {
                reason: "environment view is unusable for meteors",
            };
            return meteor_frame;
        }
        self.context_state = ContextState::Available { context };
        let now_s = self.now_s();
        let radius_m = view.game_planet_radius_m;
        self.cycle_events(&mut meteor_frame, &view, scale, radius_m, now_s);
        meteor_frame
    }

    /// Expires finished events, initializes deferred bases and pushes one
    /// validated sample set per active event through the E047-01 contract.
    fn cycle_events(
        &mut self,
        meteor_frame: &mut MeteorFrame,
        view: &RenderViewFrame,
        scale: PlanetSurfaceScale,
        radius_m: f64,
        now_s: f64,
    ) {
        let mut index = 0;
        while index < self.active.len() {
            let event = &mut self.active[index];
            if event.basis.is_none() && event.init_basis(view, scale).is_err() {
                self.active.remove(index);
                self.dismissed_total = self.dismissed_total.wrapping_add(1);
                continue;
            }
            let Some(basis) = event.basis else {
                self.active.remove(index);
                self.dismissed_total = self.dismissed_total.wrapping_add(1);
                continue;
            };
            let age_s = (now_s - event.start_s).max(0.0);
            if age_s >= event.duration_s {
                self.active.remove(index);
                self.dismissed_total = self.dismissed_total.wrapping_add(1);
                continue;
            }
            let fraction = (age_s / event.duration_s).clamp(0.0, 1.0);
            let position = basis.position_at(age_s);
            let Some(radius_phys_m) = magnitude(position) else {
                self.active.remove(index);
                self.dismissed_total = self.dismissed_total.wrapping_add(1);
                continue;
            };
            if radius_phys_m < radius_m {
                // Below the planet datum the contract forbids every sample;
                // the deterministic track ends here instead.
                self.active.remove(index);
                self.dismissed_total = self.dismissed_total.wrapping_add(1);
                continue;
            }
            let phase = phase_of(fraction);
            event.phase = phase;
            let sample = MeteorSample::new(
                event.id,
                MeteorSource::Cosmetic,
                event.start_s,
                position,
                basis.velocity_m_s,
                event.params.linear_rgb,
                event.params.intensity * intensity_envelope(fraction),
                event.params.radius_m,
                phase,
            );
            match sample {
                Ok(sample) => {
                    if !meteor_frame.push_core(sample) {
                        // Duplicate identity, cap or precision rejection: the
                        // manual stream stays bounded and dismisses the event.
                        self.active.remove(index);
                        self.dismissed_total = self.dismissed_total.wrapping_add(1);
                        continue;
                    }
                }
                // Contract-invalid constant parameters are unreachable by
                // construction; the event is dropped defensively anyway.
                Err(_) => {
                    self.active.remove(index);
                    self.dismissed_total = self.dismissed_total.wrapping_add(1);
                    continue;
                }
            }
            push_trail_slots(meteor_frame, event, basis, radius_m, now_s);
            if let Some(light) = event.params.light {
                let remaining_s = (event.duration_s - age_s).max(0.0);
                if let Ok(light_sample) = TransientLightSample::new(
                    event.id,
                    position,
                    event.params.linear_rgb,
                    light.intensity * intensity_envelope(fraction),
                    light.range_m,
                    remaining_s,
                ) {
                    let _ = meteor_frame.push_light(light_sample);
                }
            }
            index += 1;
        }
    }

    /// Dismisses every active event into the counters (expired, culled or
    /// cleared events all share the single "dismissed" bucket).
    fn reset_active(&mut self) {
        self.dismissed_total = self.dismissed_total.wrapping_add(self.active.len() as u64);
        self.active.clear();
    }

    /// Deterministic seed: monotonic birth microseconds mixed with the spawn
    /// counter; identical inputs regenerate identical tracks (no entropy).
    fn derive_seed(&self, now_s: f64) -> u64 {
        let micros = (now_s * 1.0e6).floor();
        let now_us = if micros.is_finite() && micros >= 0.0 {
            micros as u64
        } else {
            0
        };
        let mut state = now_us ^ self.spawned_total.rotate_left(32);
        splitmix64(&mut state)
    }

    /// Console status: counters, explicit context reason, seed and event ages
    /// (lifetimes stay bounded; panic-free in every state).
    pub(in crate::renderer) fn status_lines(&self) -> Vec<String> {
        let mut lines = Vec::with_capacity(super::MAX_ACTIVE_METEOR_EVENTS + 4);
        lines.push(format!(
            "meteors: manual client-local runtime; active {}/{}, spawned {}, dismissed {}",
            self.active.len(),
            super::MAX_ACTIVE_METEOR_EVENTS,
            self.spawned_total,
            self.dismissed_total
        ));
        lines.push(match self.context_state {
            ContextState::Pending => "meteors: environment context: none yet".to_owned(),
            ContextState::Available { context } => {
                format!("meteors: environment context: available ({context:?})")
            }
            ContextState::Unavailable { reason } => {
                format!("meteors: environment context: unavailable ({reason})")
            }
        });
        lines.push(format!(
            "meteors: last spawn seed 0x{:016x}",
            self.last_seed
        ));
        lines.push(
            "meteors: nearest expected event: manual only (sky meteors spawn); the automatic generator lands with E047-04"
                .to_owned(),
        );
        let now_s = self.now_s();
        for event in &self.active {
            let age_s = (now_s - event.start_s).max(0.0);
            lines.push(format!(
                "meteor {} {}: age {:.1}s, lifetime {:.1}s, phase {}, track {}",
                event.id,
                event.preset.label(),
                age_s,
                event.duration_s,
                event.phase.label(),
                if event.basis.is_some() {
                    "initialized"
                } else {
                    "awaiting environment"
                }
            ));
        }
        lines
    }
}

impl EventBasis {
    /// Linear physical integration from the single start datum.
    pub(super) fn position_at(&self, offset_s: f64) -> [f64; 3] {
        [
            self.start_phys_m[0] + self.velocity_m_s[0] * offset_s,
            self.start_phys_m[1] + self.velocity_m_s[1] * offset_s,
            self.start_phys_m[2] + self.velocity_m_s[2] * offset_s,
        ]
    }
}

/// Reconstructs elapsed trail samples in each fresh frame. Capture times and
/// positions remain deterministic, including after skipped frames or rebases.
fn push_trail_slots(
    meteor_frame: &mut MeteorFrame,
    event: &mut ActiveEvent,
    basis: EventBasis,
    radius_m: f64,
    now_s: f64,
) {
    for slot in &event.trail_slots {
        if slot.at_s > now_s {
            continue;
        }
        let position = basis.position_at(slot.at_s - event.start_s);
        if magnitude(position).is_some_and(|value| value >= radius_m) {
            // Contract-invalid constant parameters are unreachable by
            // construction; a contract-invalid sample is dropped in place.
            if let Ok(trail) = MeteorTrailSample::new(
                event.id,
                slot.serial,
                slot.at_s,
                position,
                event.params.trail_thickness_m,
                event.params.trail_opacity,
            ) {
                let _ = meteor_frame.push_trail(trail);
            }
        }
    }
}

impl ActiveEvent {
    /// Derives the deterministic flight basis from the current view and seed.
    /// The start sits above the observer column; the track descends through
    /// the seeded sky column with a bounded lateral tilt, ending at the planet
    /// datum or the planned lifetime, whichever comes first.
    fn init_basis(
        &mut self,
        view: &RenderViewFrame,
        scale: PlanetSurfaceScale,
    ) -> Result<(), &'static str> {
        let eye = view.camera_eye_world_m;
        if !eye.iter().all(|value| value.is_finite()) {
            return Err("camera eye is non-finite");
        }
        let Some(eye_radial_m) = magnitude(eye) else {
            return Err("camera eye is unusable for meteor placement");
        };
        let radius_m = view.game_planet_radius_m;
        if !radius_m.is_finite() || radius_m <= 0.0 {
            return Err("planet radius is unusable for meteor placement");
        }
        let altitude_phys_m = scale.physical_altitude_m(eye_radial_m - radius_m);
        if !altitude_phys_m.is_finite() {
            return Err("surface scale is unusable for meteor placement");
        }
        let up = [
            eye[0] / eye_radial_m,
            eye[1] / eye_radial_m,
            eye[2] / eye_radial_m,
        ];
        let axis = if up[2].abs() < 0.9 {
            [0.0, 0.0, 1.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let Some(first) = normalized(cross(up, axis)) else {
            return Err("meteor basis is degenerate");
        };
        let Some(second) = normalized(cross(up, first)) else {
            return Err("meteor basis is degenerate");
        };
        let mut rng = Rng { state: self.seed };
        let azimuth = rng.next_f64() * TAU;
        // 12°..30° off the vertical: near-vertical fall with lateral drift.
        let sink = 0.21 + rng.next_f64() * 0.31;
        let height_factor = 0.45 + rng.next_f64() * 0.2;
        let speed_m_s = self.params.speed_m_s * (0.9 + rng.next_f64() * 0.2);
        if !speed_m_s.is_finite() {
            return Err("meteor speed is finite only after clamping");
        }
        let (sin_azimuth, cos_azimuth) = azimuth.sin_cos();
        let (sin_sink, cos_sink) = sink.sin_cos();
        let bearing = [
            first[0] * cos_azimuth + second[0] * sin_azimuth,
            first[1] * cos_azimuth + second[1] * sin_azimuth,
            first[2] * cos_azimuth + second[2] * sin_azimuth,
        ];
        let velocity_m_s = [
            speed_m_s * (bearing[0] * sin_sink - up[0] * cos_sink),
            speed_m_s * (bearing[1] * sin_sink - up[1] * cos_sink),
            speed_m_s * (bearing[2] * sin_sink - up[2] * cos_sink),
        ];
        let start_radius_m =
            radius_m + altitude_phys_m + speed_m_s * self.duration_s * height_factor;
        if !start_radius_m.is_finite() || start_radius_m.abs() > POSITION_ABS_M_MAX {
            return Err("meteor start radius leaves the contract bounds");
        }
        let start_phys_m = [
            up[0] * start_radius_m,
            up[1] * start_radius_m,
            up[2] * start_radius_m,
        ];
        let slot_spacing_s = self.duration_s / (TRAIL_SLOTS_PER_EVENT + 1) as f64;
        for (slot_index, slot) in self.trail_slots.iter_mut().enumerate() {
            *slot = TrailSlot {
                at_s: self.start_s + slot_spacing_s * (slot_index + 1) as f64,
                serial: slot_index as u32,
            };
        }
        self.basis = Some(EventBasis {
            start_phys_m,
            velocity_m_s,
        });
        Ok(())
    }
}

/// Producer-declared phase from the fraction of planned lifetime consumed.
fn phase_of(fraction: f64) -> MeteorPhase {
    if fraction < 0.35 {
        MeteorPhase::Incoming
    } else if fraction < 0.75 {
        MeteorPhase::Peak
    } else {
        MeteorPhase::Fading
    }
}

/// Cubic decay keeps the emission inside the contract bounds and fades both
/// the core and the carried light smoothly before expiry.
fn intensity_envelope(fraction: f64) -> f64 {
    (1.0 - fraction).powi(3)
}

/// `None` for zero vectors and non-finite components.
fn magnitude(vector: [f64; 3]) -> Option<f64> {
    let squared = vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2];
    let value = squared.sqrt();
    (value.is_finite() && value > 0.0).then_some(value)
}

/// `None` for zero vectors and non-finite components.
fn normalized(vector: [f64; 3]) -> Option<[f64; 3]> {
    magnitude(vector).map(|value| [vector[0] / value, vector[1] / value, vector[2] / value])
}

/// World-right-handed exterior product of two vectors.
fn cross(first: [f64; 3], second: [f64; 3]) -> [f64; 3] {
    [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ]
}

/// Deterministic splitmix64 step; the only randomness source of this runtime.
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_8CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Fixed-width deterministic stream over one event seed.
struct Rng {
    state: u64,
}

impl Rng {
    /// Uniform value in `[0, 1)`; 53 mantissa bits are consumed per call.
    fn next_f64(&mut self) -> f64 {
        let raw = splitmix64(&mut self.state) >> 11;
        raw as f64 / (1u64 << 53) as f64
    }
}
