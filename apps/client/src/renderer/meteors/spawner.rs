//! Bounded, counter-based scheduling; UTC and frame rate never drive births.
use super::track::splitmix64;
use crate::client_commands::MeteorPreset;
use minimium_protocol::WorldContext;
use std::hash::{Hash, Hasher};

const SLOT_S: f64 = 210.0;
const JITTER_S: f64 = 90.0;

pub(super) struct Candidate {
    pub seed: u64,
    pub at_s: f64,
    pub preset: MeteorPreset,
}

pub(super) struct Spawner {
    pub enabled: bool,
    pub rate: f64,
    pub seed: u64,
    session_seed: u64,
    initial_seed: u64,
    context: Option<WorldContext>,
    start_s: f64,
    next_slot: u64,
    pub spawned: u64,
    pub skipped: u64,
    pub peak: usize,
    pub reason: &'static str,
}

impl Spawner {
    pub fn new() -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        let session_seed = nanos ^ u64::from(std::process::id()).rotate_left(32);
        Self {
            enabled: true,
            rate: 1.0,
            seed: session_seed,
            session_seed,
            initial_seed: session_seed,
            context: None,
            start_s: 0.0,
            next_slot: 0,
            spawned: 0,
            skipped: 0,
            peak: 0,
            reason: "no_environment",
        }
    }

    pub fn seed_override(&mut self, seed: Option<u64>) {
        self.session_seed = seed.unwrap_or(self.initial_seed);
        self.suspend();
    }

    pub fn suspend(&mut self) {
        self.context = None;
        self.reason = "no_environment";
    }

    fn word(&self, index: u64) -> u64 {
        splitmix64(&mut (self.seed ^ index.wrapping_mul(0x9e3779b97f4a7c15)))
    }

    fn at(&self, index: u64) -> f64 {
        let jitter = (self.word(index) >> 11) as f64 / (1u64 << 53) as f64;
        self.start_s + (index as f64 * SLOT_S + JITTER_S * jitter) / self.rate.max(0.001)
    }

    pub fn next_in_s(&self, now_s: f64) -> Option<f64> {
        (self.enabled && self.rate > 0.0 && self.context.is_some())
            .then(|| (self.at(self.next_slot) - now_s).max(0.0))
    }

    pub fn advance(&mut self, context: WorldContext, now_s: f64) -> Option<Candidate> {
        if !self.enabled || self.rate == 0.0 {
            self.suspend();
            self.reason = "disabled";
            return None;
        }
        if self.context != Some(context) {
            // WorldContext has a stable, integer-only Hash representation in
            // this build. No randomized HashMap state enters the seed.
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            context.hash(&mut hash);
            self.seed = self.session_seed ^ hash.finish();
            self.context = Some(context);
            self.start_s = now_s + 120.0 / self.rate;
            self.next_slot = 0;
        }
        self.reason = "waiting_monotonic_deadline";
        if now_s < self.at(self.next_slot) {
            return None;
        }
        let mut slot = ((now_s - self.start_s) * self.rate / SLOT_S)
            .max(0.0)
            .floor() as u64;
        if self.at(slot) > now_s {
            slot = slot.saturating_sub(1);
        }
        self.skipped = self
            .skipped
            .saturating_add(slot.saturating_sub(self.next_slot));
        self.next_slot = slot.saturating_add(1);
        let seed = splitmix64(&mut self.word(slot));
        let preset = match seed % 1000 {
            0 => MeteorPreset::Fireball,
            1..=50 => MeteorPreset::Bright,
            _ => MeteorPreset::Faint,
        };
        Some(Candidate {
            seed,
            at_s: self.at(slot),
            preset,
        })
    }
}
