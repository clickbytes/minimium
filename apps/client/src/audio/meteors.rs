//! Acoustic scheduling in SI metres and the meteor client monotonic clock.
use glam::DVec3;

pub const QUEUE_CAPACITY: usize = 16;
pub const MAX_DELAY_S: f64 = 600.0;
#[derive(Clone, Copy, Debug)]
pub enum CueKind {
    Airburst,
}
#[derive(Clone, Copy, Debug)]
pub struct MeteorSoundCue {
    pub source_pos: [f64; 3],
    pub onset_monotonic: f64,
    pub relative_energy: f32,
    pub cue_kind: CueKind,
    pub seed: u64,
}
#[derive(Clone, Copy)]
pub struct PendingCue {
    pub cue: MeteorSoundCue,
    pub arrival_s: f64,
    pub gain: f32,
    pub pan: f32,
}
impl PendingCue {
    pub fn new(cue: MeteorSoundCue, listener: [f64; 3], right: [f64; 3]) -> Option<Self> {
        if !cue.relative_energy.is_finite() || !DVec3::from_array(right).is_finite() {
            return None;
        }
        let delta = DVec3::from_array(cue.source_pos) - DVec3::from_array(listener);
        let distance = delta.length();
        // Homogeneous 15 C dry air: c ~=331.3+0.606*T; no refraction/wind solver.
        let delay = distance / (331.3 + 0.606 * 15.0);
        if !delay.is_finite() || delay > MAX_DELAY_S || !cue.onset_monotonic.is_finite() {
            return None;
        }
        let pan = delta
            .normalize_or_zero()
            .dot(DVec3::from_array(right).normalize_or_zero())
            .clamp(-1.0, 1.0) as f32;
        let gain =
            cue.relative_energy.clamp(0.0, 1.0) / (1.0 + (distance / 30_000.0).powi(2)) as f32;
        Some(Self {
            cue,
            arrival_s: cue.onset_monotonic + delay,
            gain,
            pan,
        })
    }
}
