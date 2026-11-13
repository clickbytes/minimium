//! Manual meteor lifecycle mechanics: frame building, spawn/clear and status.

use super::{ActiveEvent, ContextState, MeteorRuntime};
use crate::client_commands::MeteorPreset;
use crate::renderer::environment::{EnvironmentFrame, EnvironmentFrameStatus, RenderViewFrame};
use minimium_renderer::meteors::{
    coords::PlanetSurfaceScale, MeteorDisabledReason, MeteorFrame, MeteorFrameContext,
    MeteorFrameState,
};

impl MeteorRuntime {
    /// Creates the runtime with its single monotonic datum.
    pub(in crate::renderer) fn new() -> Self {
        Self {
            audio: crate::audio::MeteorAudio::default(),
            seed_override: None,
            quality: minimium_renderer::quality::SkyQuality::Standard,
            epoch: std::time::Instant::now(),
            next_id: 1,
            active: Vec::with_capacity(super::MAX_ACTIVE_METEOR_EVENTS),
            spawned_total: 0,
            dismissed_total: 0,
            last_seed: 0,
            context: None,
            context_state: ContextState::Pending,
            spawner: super::spawner::Spawner::new(),
            history_pool: (0..super::MAX_ACTIVE_METEOR_EVENTS)
                .map(|_| super::evolution::Evolution::empty())
                .collect(),
            omitted_cores: 0,
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
    pub(in crate::renderer) fn spawn(
        &mut self,
        preset: MeteorPreset,
        seed: Option<u64>,
    ) -> Result<String, String> {
        if !self.spawner.enabled {
            return Err("meteors: disabled by client.sky.meteors.enabled".into());
        }
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
        self.next_id = self.next_id % (u64::MAX / super::EVENT_ID_STRIDE) + 1;
        let seed = seed
            .or(self.seed_override)
            .unwrap_or_else(|| self.derive_seed(now_s));
        let allow_long = self.allow_long_train();
        let history = self
            .history_pool
            .pop()
            .ok_or("meteors: history pool exhausted")?;
        self.active.push(ActiveEvent::new(
            id, preset, seed, now_s, history, allow_long,
        ));
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
        if let Some(candidate) = self.spawner.advance(context, now_s) {
            if now_s - candidate.at_s >= 20.0 {
                self.spawner.skipped = self.spawner.skipped.saturating_add(1);
                self.spawner.reason = "expired_during_gap";
            } else if self.active.len() >= super::MAX_ACTIVE_METEOR_EVENTS {
                self.spawner.skipped = self.spawner.skipped.saturating_add(1);
                self.spawner.reason = "active_cap";
            } else {
                let allow_long = self.allow_long_train();
                let mut event = ActiveEvent::new(
                    self.next_id,
                    candidate.preset,
                    candidate.seed,
                    candidate.at_s,
                    self.history_pool
                        .pop()
                        .expect("active cap reserves a history"),
                    allow_long,
                );
                event.automatic = true;
                self.next_id = self.next_id % (u64::MAX / super::EVENT_ID_STRIDE) + 1;
                self.active.push(event);
                self.spawner.spawned = self.spawner.spawned.saturating_add(1);
                self.spawned_total = self.spawned_total.wrapping_add(1);
                self.spawner.reason = "candidate_admitted_visibility_independent";
            }
        }
        self.audio.observe(super::sounds::listener(&view, scale));
        self.cycle_events(&mut meteor_frame, &view, scale, now_s);
        self.audio.tick(now_s);
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
        now_s: f64,
    ) {
        let mut index = 0;
        while index < self.active.len() {
            let event = &mut self.active[index];
            if event.basis.is_none() && event.init_basis(view, scale).is_err() {
                self.dismiss(index);
                continue;
            }
            if event.evolution.is_none() {
                self.dismiss(index);
                continue;
            }
            let age_s = (now_s - event.start_s).max(0.0);
            if age_s >= event.duration_s {
                self.dismiss(index);
                continue;
            }
            super::sounds::capture(event, &mut self.audio, view, scale, now_s);
            let history = event.evolution.as_ref().expect("initialized above");
            event.trains.advance(history, age_s);
            event.phase = history.phase(age_s, 0);
            self.omitted_cores +=
                super::presentation::push_event(meteor_frame, event, age_s, false);
            index += 1;
        }
        for event in &self.active {
            let age_s = (now_s - event.start_s).max(0.0);
            self.omitted_cores += super::presentation::push_event(meteor_frame, event, age_s, true);
            self.omitted_cores +=
                super::trains::push(meteor_frame, event, age_s, view.game_planet_radius_m);
        }
    }

    /// Dismisses every active event into the counters (expired, culled or
    /// cleared events all share the single "dismissed" bucket).
    fn reset_active(&mut self) {
        self.audio.clear();
        self.dismissed_total = self.dismissed_total.wrapping_add(self.active.len() as u64);
        for mut event in self.active.drain(..) {
            if let Some(history) = event.evolution.take() {
                self.history_pool.push(history);
            }
        }
    }

    fn allow_long_train(&self) -> bool {
        self.active
            .iter()
            .filter(|event| event.trains.profile.glow_ttl_s > 3.0)
            .count()
            < 2
    }

    fn dismiss(&mut self, index: usize) {
        let mut event = self.active.remove(index);
        if let Some(history) = event.evolution.take() {
            self.history_pool.push(history);
        }
        self.dismissed_total = self.dismissed_total.wrapping_add(1);
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
            "meteor controls: enabled {}, rate {}, seed_override {:?}, requested quality {}",
            self.spawner.enabled,
            self.spawner.rate,
            self.seed_override,
            self.quality.label()
        ));
        lines.push(self.audio.status());
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
            "meteors auto: enabled {}, seed {}, base interval 120..300 real s, next {:?}s, spawned {}, skipped {}, peak {}, gate {}",
            self.spawner.enabled,
            self.spawner.seed,
            self.spawner.next_in_s(now_s),
            self.spawner.spawned,
            self.spawner.skipped,
            self.spawner.peak,
            self.spawner.reason,
        ));
        lines.push(format!(
            "meteors budgets: rejected core submissions {}, free history slots {}/8, history sample bytes {}",
            self.omitted_cores,
            self.history_pool.len(),
            8 * minimium_renderer::meteors::MAX_METEOR_HISTORY_SAMPLES
                * minimium_renderer::meteors::fragmentation::BODY_SLOTS
                * std::mem::size_of::<minimium_renderer::meteors::kinematics::Body>()
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
            if let Some(history) = event.evolution.as_ref().filter(|_| event.basis.is_some()) {
                let body = history.sample(age_s);
                lines.push(format!(
                    "  train points {}, overwritten {}, skipped captures {}, glow TTL {:.1}s, dust TTL {:.1}s",
                    event.trains.point_count(), event.trains.overwritten(), event.trains.skipped_samples,
                    event.trains.profile.glow_ttl_s, event.trains.profile.dust_ttl_s,
                ));
                lines.push(format!(
                    "  material {:?}, mass {:.6} kg, speed {:.1} m/s, solid radius {:.5} m, emission {:.2} W",
                    history.material.kind, body.mass_kg, body.velocity_m_s.length(),
                    body.radius_m(history.material), body.gas_power_w,
                ));
                lines.push(format!(
                    "  final mass ledger solid/dust packets/unrepresented dust/ablated {:?} kg; residual {:.9} kg; grain packets {}; events {}/64 omitted {}",
                    history.mass_ledger,
                    event.params.mass_kg - history.mass_ledger.iter().sum::<f64>(),
                    history.grain_packets, history.event_count, history.omitted_events,
                ));
                for process in history.events[..history.event_count]
                    .iter()
                    .filter(|process| process.at_s <= age_s)
                    .rev()
                    .take(4)
                {
                    lines.push(format!(
                        "  {:?} at {:.3}s slot {} transferred {:.9} kg",
                        process.kind, process.at_s, process.slot, process.mass_kg
                    ));
                }
            }
        }
        lines
    }
}

/// `None` for zero vectors and non-finite components.
pub(super) fn magnitude(vector: [f64; 3]) -> Option<f64> {
    let squared = vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2];
    let value = squared.sqrt();
    (value.is_finite() && value > 0.0).then_some(value)
}
