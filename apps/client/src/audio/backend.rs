//! Rodio/CPAL lives on one owned worker, with bounded commands and four voices.
use super::meteors::PendingCue;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering},
    mpsc::{self, SyncSender},
    Arc,
};
use std::time::Duration;
struct Shared {
    stop: AtomicBool,
    generation: AtomicU64,
    status: AtomicU8,
    volume: AtomicU32,
}
pub struct Backend {
    shared: Arc<Shared>,
    sender: SyncSender<(u64, PendingCue)>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Backend {
    pub fn spawn() -> Option<Self> {
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            status: AtomicU8::new(1),
            volume: AtomicU32::new(0.25f32.to_bits()),
        });
        let (sender, receiver) = mpsc::sync_channel::<(u64, PendingCue)>(8);
        let state = shared.clone();
        let worker = std::thread::Builder::new()
            .name("meteor-audio".into())
            .spawn(move || {
                let Ok(stream) = rodio::DeviceSinkBuilder::open_default_sink() else {
                    state.status.store(3, Ordering::Release);
                    return;
                };
                state.status.store(2, Ordering::Release);
                let mut generation = state.generation.load(Ordering::Acquire);
                let mut voices: Vec<rodio::Player> = Vec::with_capacity(4);
                while !state.stop.load(Ordering::Acquire) {
                    let current = state.generation.load(Ordering::Acquire);
                    if current != generation {
                        voices.clear();
                        generation = current;
                    }
                    voices.retain(|voice| !voice.empty());
                    for voice in &voices {
                        voice.set_volume(f32::from_bits(state.volume.load(Ordering::Relaxed)));
                    }
                    match receiver.recv_timeout(Duration::from_millis(25)) {
                        Ok((stamp, cue)) if stamp == generation && voices.len() < 4 => {
                            let data = synthesize(cue);
                            if stamp != state.generation.load(Ordering::Acquire) {
                                continue;
                            }
                            let voice = rodio::Player::connect_new(stream.mixer());
                            voice.set_volume(f32::from_bits(state.volume.load(Ordering::Relaxed)));
                            voice.append(rodio::buffer::SamplesBuffer::new(
                                rodio::nz!(2),
                                rodio::nz!(24000),
                                data,
                            ));
                            voices.push(voice);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        _ => {}
                    }
                }
                voices.clear();
                state.status.store(4, Ordering::Release);
            })
            .ok()?;
        Some(Self {
            shared,
            sender,
            worker: Some(worker),
        })
    }
    pub fn play(&self, cue: PendingCue, volume: f32) -> bool {
        self.set_volume(volume);
        self.shared.status.load(Ordering::Acquire) == 2
            && self
                .sender
                .try_send((self.shared.generation.load(Ordering::Acquire), cue))
                .is_ok()
    }
    pub fn clear(&self) {
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
    }
    pub fn set_volume(&self, volume: f32) {
        self.shared
            .volume
            .store(volume.to_bits(), Ordering::Relaxed);
    }
    pub fn status(&self) -> &'static str {
        match self.shared.status.load(Ordering::Acquire) {
            1 => "initializing",
            2 => "ready",
            3 => "device unavailable",
            _ => "stopped",
        }
    }
}
impl Drop for Backend {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
fn synthesize(cue: PendingCue) -> Vec<f32> {
    // Original seeded rumble: no recordings/codecs; 3 seconds, stereo, 24kHz.
    let frequency = match cue.cue.cue_kind {
        super::meteors::CueKind::Airburst => 45.0,
    };
    let mut samples = Vec::with_capacity(144000);
    let mut seed = cue.cue.seed | 1;
    let mut filtered = 0.0f32;
    let left = ((1.0 - cue.pan) * 0.5).sqrt();
    let right = ((1.0 + cue.pan) * 0.5).sqrt();
    for index in 0..72000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let noise = (seed >> 40) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0;
        filtered += 0.025 * (noise - filtered);
        let t = index as f32 / 24000.0;
        let envelope = (t / 0.04).min(1.0) * (-t * 1.7).exp() * ((3.0 - t) / 0.2).clamp(0.0, 1.0);
        let wave =
            (filtered * 4.0 + 0.2 * (t * frequency * std::f32::consts::TAU).sin()).clamp(-1.0, 1.0);
        let sample = wave * envelope * cue.gain * 0.08;
        samples.push(sample * left);
        samples.push(sample * right);
    }
    samples
}
