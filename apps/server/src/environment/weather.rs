//! Asynchronously reconstructed global weather, derived only from server world
//! time and world seed. The source is also available to client reconstruction.
use crate::world_registry::WorldRegistry;
use minimium_core::{
    environment::EnvironmentProfile,
    time::ClockSample,
    world::WorldInstanceId,
};
use minimium_weather::{AtmosphereProfile, WeatherGrid, WeatherSurface};
use std::{
    io,
    sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
    thread,
};

const FIELD_EPOCH_MS: i64 = 600_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Request {
    world: WorldInstanceId,
    seed: u128,
    utc_ms: i64,
    clock_revision: u64,
}

pub(crate) struct WeatherAuthority {
    sender: SyncSender<Request>,
    receiver: Receiver<(Request, WeatherGrid)>,
    wanted: Option<Request>,
    published: Option<(Request, WeatherGrid)>,
    failed: bool,
}

impl WeatherAuthority {
    pub(crate) fn new() -> io::Result<Self> {
        let (sender, jobs) = mpsc::sync_channel::<Request>(1);
        let (results, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("server-global-weather".into())
            .spawn(move || {
                let surface = WeatherSurface::earth();
                while let Ok(mut job) = jobs.recv() {
                    while let Ok(next) = jobs.try_recv() {
                        job = next;
                    }
                    let frame = WeatherGrid::reconstruct(
                        job.seed, job.utc_ms, AtmosphereProfile::earth(), &surface,
                    );
                    if results.send((job, frame)).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            sender,
            receiver,
            wanted: None,
            published: None,
            failed: false,
        })
    }

    pub(crate) fn update(&mut self, worlds: &WorldRegistry, clock: ClockSample) {
        // Earth only at this milestone. Future planet profiles will have one
        // independently keyed state per WorldInstanceId.
        let target = worlds.iter().find_map(|(world, instance)| {
            (instance.environment_profile == EnvironmentProfile::Earth).then(|| Request {
                world,
                seed: instance.manifest.generation().seed().get(),
                utc_ms: clock.world_utc_ms().get().div_euclid(FIELD_EPOCH_MS) * FIELD_EPOCH_MS,
                clock_revision: clock.revision().get(),
            })
        });
        let Some(target) = target else { return; };
        let previous = self.wanted;
        self.wanted = Some(target);
        loop {
            match self.receiver.try_recv() {
                Ok((request, frame)) => {
                    if request == target {
                        self.published = Some((request, frame));
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.failed = true;
                    break;
                }
            }
        }
        if self.failed || self.published.as_ref().is_some_and(|(key, _)| *key == target) {
            return;
        }
        if previous != Some(target) {
            match self.sender.try_send(target) {
                Ok(()) | Err(TrySendError::Full(_)) => {}
                Err(TrySendError::Disconnected(_)) => self.failed = true,
            }
        }
    }

    pub(crate) fn status(&self) -> String {
        let status = if self.failed { "worker_failed" }
            else if self.wanted.is_some() && self.published.as_ref().map(|(key, _)| *key) == self.wanted {
                "ready"
            } else { "preparing" };
        match &self.published {
            Some((key, field)) => {
                let sample = field.sample(0.9, 0.4, 0);
                format!("weather_authority={status}:model={} world={} seed={} UTC={} T={:.1}K RH={:.2}",
                    minimium_weather::MODEL_REVISION, key.world, key.seed,
                    key.utc_ms, sample.temperature_k, sample.humidity())
            }
            None => format!("weather_authority={status}:no_published_field"),
        }
    }
}
