//! Manual meteor lifecycle mechanics: frame building, spawn/clear and status.

use super::{ActiveEvent, ContextState, MeteorRuntime};
use crate::client_commands::MeteorPreset;
use crate::renderer::environment::{EnvironmentFrame, EnvironmentFrameStatus, RenderViewFrame};
use minimium_renderer::meteors::{
    coords::PlanetSurfaceScale,
    model::{MeteorSample, MeteorSource, MeteorTrailSample, TransientLightSample},
    MeteorDisabledReason, MeteorFrame, MeteorFrameContext, MeteorFrameState,
};

impl MeteorRuntime {
    /// Creates the runtime with its single monotonic datum.
    pub(in crate::renderer) fn new() -> Self {
        Self {
            epoch: std::time::Instant::now(),
            next_id: 1,
            active: Vec::with_capacity(super::MAX_ACTIVE_METEOR_EVENTS),
            spawned_total: 0,
            dismissed_total: 0,
            last_seed: 0,
            context: None,
            context_state: ContextState::Pending,
            spawner: super::spawner::Spawner::new(),
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
        if !matches!(frame.status, EnvironmentFrameStatus::Available) || frame.context.is_none() {
            self.spawner.suspend();
        }
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
            self.spawner.suspend();
            self.reset_active();
            self.context_state = ContextState::Unavailable {
                reason: "environment view is unusable for meteors",
            };
            return meteor_frame;
        }
        self.context_state = ContextState::Available { context };
        let now_s = self.now_s();
        let radius_m = view.game_planet_radius_m;
        if let Some(candidate) = self.spawner.advance(context, now_s) {
            if now_s - candidate.at_s >= 20.0 {
                self.spawner.skipped = self.spawner.skipped.saturating_add(1);
                self.spawner.reason = "expired_during_gap";
            } else if self.active.len() >= super::MAX_ACTIVE_METEOR_EVENTS {
                self.spawner.skipped = self.spawner.skipped.saturating_add(1);
                self.spawner.reason = "active_cap";
            } else {
                let mut event = ActiveEvent::new(
                    self.next_id,
                    candidate.preset,
                    candidate.seed,
                    candidate.at_s,
                );
                event.automatic = true;
                self.next_id = self.next_id.wrapping_add(1).max(1);
                self.active.push(event);
                self.spawner.spawned = self.spawner.spawned.saturating_add(1);
                self.spawned_total = self.spawned_total.wrapping_add(1);
                self.spawner.reason = "candidate_admitted_visibility_independent";
            }
        }
        self.cycle_events(&mut meteor_frame, &view, scale, radius_m, now_s);
        self.spawner.peak = self.spawner.peak.max(self.active.len());
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
            let Some(history) = event.evolution.as_ref() else {
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
            let body = history.sample(age_s);
            let emission = minimium_renderer::meteors::photometry::emission(
                body,
                history.material,
                age_s,
                event.seed,
            );
            let position = body.position_m.to_array();
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
            let phase = history.phase(age_s);
            event.phase = phase;
            let sample = MeteorSample::new(
                event.id,
                MeteorSource::Cosmetic,
                event.start_s,
                position,
                body.velocity_m_s.to_array(),
                emission.rgb,
                emission.intensity,
                emission.radius_m,
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
            push_trail_slots(meteor_frame, event, radius_m, now_s);
            if let Some(light) = event.params.light {
                let remaining_s = (event.duration_s - age_s).max(0.0);
                if let Ok(light_sample) = TransientLightSample::new(
                    event.id,
                    position,
                    emission.rgb,
                    emission.intensity,
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
        super::track::splitmix64(&mut state)
    }

    /// Console status: counters, explicit context reason, seed and event ages
    /// (lifetimes stay bounded; panic-free in every state).
    pub(in crate::renderer) fn status_lines(&self) -> Vec<String> {
        let mut lines = Vec::with_capacity(super::MAX_ACTIVE_METEOR_EVENTS + 4);
        lines.push(format!(
            "meteors: client-local runtime; active {}/{}, spawned {}, dismissed {}",
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
        let now_s = self.now_s();
        lines.push(format!(
            "meteors auto: enabled {}, seed {}, interval 120..300 real s, next {:?}s, spawned {}, skipped {}, peak {}, gate {}",
            self.spawner.enabled,
            self.spawner.seed,
            self.spawner.next_in_s(now_s),
            self.spawner.spawned,
            self.spawner.skipped,
            self.spawner.peak,
            self.spawner.reason,
        ));
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
            if let Some(history) = &event.evolution {
                let body = history.sample(age_s);
                lines.push(format!(
                    "  material {:?}, mass {:.6} kg, speed {:.1} m/s, solid radius {:.5} m, emission {:.2} W",
                    history.material.kind, body.mass_kg, body.velocity_m_s.length(),
                    body.radius_m(history.material), body.gas_power_w,
                ));
            }
        }
        lines
    }
}

/// Reconstructs elapsed trail samples in each fresh frame. Capture times and
/// positions remain deterministic, including after skipped frames or rebases.
fn push_trail_slots(
    meteor_frame: &mut MeteorFrame,
    event: &ActiveEvent,
    radius_m: f64,
    now_s: f64,
) {
    let Some(history) = &event.evolution else {
        return;
    };
    for slot in &event.trail_slots {
        if slot.at_s > now_s {
            continue;
        }
        let body = history.sample(slot.at_s - event.start_s);
        let position = body.position_m.to_array();
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

/// `None` for zero vectors and non-finite components.
pub(super) fn magnitude(vector: [f64; 3]) -> Option<f64> {
    let squared = vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2];
    let value = squared.sqrt();
    (value.is_finite() && value > 0.0).then_some(value)
}
