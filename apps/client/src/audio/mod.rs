//! Optional application audio adapter; never blocks rendering on device setup.
#[cfg(feature = "desktop-audio")]
mod backend;
pub mod meteors;
use meteors::{MeteorSoundCue, PendingCue, QUEUE_CAPACITY};
use std::collections::VecDeque;

pub struct MeteorAudio {
    pub enabled: bool,
    pub volume: f32,
    queue: VecDeque<PendingCue>,
    last_listener: Option<[f64; 3]>,
    pub submitted: u64,
    pub dropped: u64,
    last_due: Option<(meteors::CueKind, u64, f32, f32)>,
    #[cfg(feature = "desktop-audio")]
    backend: Option<backend::Backend>,
}
impl Default for MeteorAudio {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0.25,
            queue: VecDeque::with_capacity(QUEUE_CAPACITY),
            last_listener: None,
            submitted: 0,
            dropped: 0,
            last_due: None,
            #[cfg(feature = "desktop-audio")]
            backend: None,
        }
    }
}
impl MeteorAudio {
    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }
        self.clear();
        self.enabled = enabled;
        #[cfg(feature = "desktop-audio")]
        if enabled
            && self
                .backend
                .as_ref()
                .is_some_and(|backend| backend.status() == "device unavailable")
        {
            self.backend = None;
        }
        #[cfg(feature = "desktop-audio")]
        if enabled && self.backend.is_none() {
            self.backend = backend::Backend::spawn();
        }
    }
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            0.0
        };
        #[cfg(feature = "desktop-audio")]
        if let Some(backend) = &self.backend {
            backend.set_volume(self.volume);
        }
        if self.volume == 0.0 {
            self.clear();
        }
    }
    pub fn clear(&mut self) {
        self.dropped += self.queue.len() as u64;
        self.queue.clear();
        self.last_listener = None;
        #[cfg(feature = "desktop-audio")]
        if let Some(backend) = &self.backend {
            backend.clear();
        }
    }
    pub fn observe(&mut self, listener: [f64; 3]) {
        if self
            .last_listener
            .is_some_and(|old| glam::DVec3::from_array(old).distance(listener.into()) > 5000.0)
        {
            self.clear();
        }
        self.last_listener = Some(listener);
    }
    pub fn enqueue(
        &mut self,
        cue: MeteorSoundCue,
        listener: [f64; 3],
        right: [f64; 3],
        now_s: f64,
    ) {
        if !self.enabled || self.volume <= 0.0 {
            return;
        }
        let Some(pending) = PendingCue::new(cue, listener, right) else {
            self.dropped += 1;
            return;
        };
        if pending.arrival_s + 2.0 < now_s || self.queue.len() == QUEUE_CAPACITY {
            self.dropped += 1;
            return;
        }
        let index = self
            .queue
            .iter()
            .position(|old| pending.arrival_s < old.arrival_s)
            .unwrap_or(self.queue.len());
        self.queue.insert(index, pending);
    }
    pub fn tick(&mut self, now_s: f64) {
        while self.queue.front().is_some_and(|cue| cue.arrival_s <= now_s) {
            let Some(cue) = self.queue.pop_front() else {
                break;
            };
            if now_s > cue.arrival_s + 2.0 || !self.enabled {
                self.dropped += 1;
                continue;
            }
            self.last_due = Some((cue.cue.cue_kind, cue.cue.seed, cue.gain, cue.pan));
            #[cfg(feature = "desktop-audio")]
            let sent = self
                .backend
                .as_ref()
                .is_some_and(|backend| backend.play(cue, self.volume));
            #[cfg(not(feature = "desktop-audio"))]
            let sent = false;
            if sent {
                self.submitted += 1;
            } else {
                self.dropped += 1;
            }
        }
    }
    pub fn backend_status(&self) -> &'static str {
        #[cfg(feature = "desktop-audio")]
        {
            self.backend
                .as_ref()
                .map_or("not initialized", |backend| backend.status())
        }
        #[cfg(not(feature = "desktop-audio"))]
        {
            "desktop-audio feature disabled"
        }
    }
    pub fn status(&self) -> String {
        format!("meteor audio: enabled {}, volume {:.2}, backend {}, queued {}/16, submitted {}, dropped {}, last_due {:?}",
            self.enabled,self.volume,self.backend_status(),self.queue.len(),self.submitted,self.dropped,self.last_due)
    }
}
