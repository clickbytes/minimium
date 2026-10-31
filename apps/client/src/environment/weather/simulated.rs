use super::{
    bootstrap,
    grid::{WeatherResolution, ATLAS_LAYERS, SNAPSHOT_LAYERS},
    maps::{self, Maps, Request, TerrainCache},
    region::Region,
    terrain::TerrainConfig,
    WeatherSample,
};
use std::sync::mpsc::{self, Receiver, Sender};

pub(super) const WIDTH: usize = 512;
pub(super) const HEIGHT: usize = 256;
pub(super) const STEP_MS: i64 = 10 * 60 * 1000;

/// Atlas construction never blocks redraw. The worker coalesces obsolete requests.
pub(super) struct SimulatedWeather {
    resolution: WeatherResolution,
    requested: Option<Request>,
    maps: Option<Maps>,
    detailed: bool,
    worker_failed: bool,
    empty: [Vec<u8>; ATLAS_LAYERS],
    blend: f32,
    lag_seconds: f64,
    correction: Option<(f64, f64, WeatherSample)>,
    sender: Sender<Request>,
    receiver: Receiver<(Maps, bool)>,
}

impl SimulatedWeather {
    pub(super) fn new() -> Self {
        let (sender, jobs) = mpsc::channel::<Request>();
        let (results, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut cache = TerrainCache::default();
            let mut previous: Option<Request> = None;
            while let Ok(mut job) = jobs.recv() {
                while let Ok(newer) = jobs.try_recv() {
                    job = newer;
                }
                let bootstrap_needed = previous.is_none_or(|old| {
                    old.seed != job.seed
                        || old.terrain != job.terrain
                        || old.resolution != job.resolution
                        || old.clock_revision != job.clock_revision
                });
                if bootstrap_needed && results.send((bootstrap::build(job), false)).is_err() {
                    break;
                }
                if results.send((cache.build(job), true)).is_err() {
                    break;
                }
                previous = Some(job);
            }
        });
        Self {
            resolution: WeatherResolution::default(),
            requested: None,
            maps: None,
            detailed: false,
            worker_failed: false,
            empty: std::array::from_fn(|layer| {
                let color = match layer % SNAPSHOT_LAYERS {
                    2 | 3 => [128, 166, 0, 0],
                    4 | 5 => [128, 128, 0, 0],
                    12..=15 => [128; 4],
                    _ => [0; 4],
                };
                color.repeat(WIDTH * HEIGHT)
            }),
            blend: 0.0,
            lag_seconds: 0.0,
            correction: None,
            sender,
            receiver,
        }
    }

    pub(super) fn set_correction(&mut self, correction: Option<(f64, f64, WeatherSample)>) {
        self.correction = correction;
    }

    pub(super) fn resolution(&self) -> WeatherResolution {
        self.resolution
    }

    pub(super) fn set_resolution(&mut self, resolution: WeatherResolution) {
        self.resolution = resolution;
    }

    pub(super) fn update(
        &mut self,
        seed: u128,
        utc_ms: i64,
        clock_revision: Option<u64>,
        terrain: Option<TerrainConfig>,
        location: Option<(f64, f64)>,
    ) -> bool {
        let request = Request {
            seed,
            clock_revision,
            epoch: utc_ms.div_euclid(STEP_MS),
            terrain,
            resolution: self.resolution,
            region: location.map_or(Region::GLOBAL, |(lat, lon)| Region::around(lat, lon)),
            correction: self.correction,
        };
        let mut changed = false;
        if self.requested != Some(request) {
            let discontinuity = self.requested.is_some_and(|old| {
                old.seed != seed
                    || old.resolution != request.resolution
                    || old.terrain != request.terrain
                    || old.clock_revision != request.clock_revision
            });
            if discontinuity {
                changed = self.maps.take().is_some();
                self.detailed = false;
            }
            let _ = self.sender.send(request);
            self.requested = Some(request);
        }
        loop {
            let (maps, detailed) = match self.receiver.try_recv() {
                Ok(result) => result,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker_failed = true;
                    break;
                }
            };
            // A completed field from this clock revision stays usable while newer
            // work catches up. At 10,000x, a ten-minute epoch lasts only 60 ms:
            // expiring by epoch distance can reject every result indefinitely.
            let mut expected = request;
            expected.epoch = maps.request.epoch;
            expected.region = maps.request.region;
            let newer = self
                .maps
                .as_ref()
                .is_none_or(|old| maps.request.epoch >= old.request.epoch);
            if maps.request == expected && newer {
                self.maps = Some(maps);
                self.detailed = detailed;
                changed = true;
            }
        }
        let epoch = self
            .maps
            .as_ref()
            .map_or(request.epoch, |m| m.request.epoch);
        self.blend = ((utc_ms as f64 / STEP_MS as f64) - epoch as f64).clamp(0.0, 1.0) as f32;
        self.lag_seconds = self.maps.as_ref().map_or(0.0, |_| {
            ((utc_ms as f64 - (epoch + 1) as f64 * STEP_MS as f64) / 1000.0).max(0.0)
        });
        changed
    }

    pub(super) fn pixels(&self) -> [&[u8]; ATLAS_LAYERS] {
        let pixels = self.maps.as_ref().map_or(&self.empty, |m| &m.pixels);
        std::array::from_fn(|i| pixels[i].as_slice())
    }

    pub(super) fn region(&self) -> [f32; 4] {
        self.maps
            .as_ref()
            .map_or([0.0; 4], |m| m.request.region.0.map(|v| v as f32))
    }

    pub(super) fn blend(&self) -> f32 {
        self.blend
    }

    pub(super) fn start_seconds(&self) -> Option<f64> {
        self.maps
            .as_ref()
            .map(|m| m.request.epoch as f64 * STEP_MS as f64 / 1000.0)
    }

    pub(super) fn status(&self) -> String {
        let state = if self.worker_failed {
            "weather worker unavailable; retaining published atlas"
        } else if self.detailed {
            "regional atmosphere ready"
        } else if self.maps.is_none() {
            "preparing global weather field"
        } else {
            "coarse global field; preparing terrain detail"
        };
        let lag = if self.lag_seconds > 1.0 {
            format!(
                " | catching up: {:.0} world seconds behind",
                self.lag_seconds
            )
        } else {
            String::new()
        };
        format!("{} | {state}{lag}", self.resolution.description())
    }

    pub(super) fn sample(&self, latitude_deg: f64, longitude_deg: f64) -> WeatherSample {
        let uv = [
            ((longitude_deg + 180.0) / 360.0).rem_euclid(1.0),
            ((90.0 - latitude_deg) / 180.0).clamp(0.0, 1.0),
        ];
        let pixels = self.maps.as_ref().map_or(&self.empty, |m| &m.pixels);
        let (local_uv, weight) = self.maps.as_ref().map_or(([0.5; 2], 0.0), |m| {
            m.request
                .region
                .uv_weight(latitude_deg.to_radians(), longitude_deg.to_radians())
        });
        let mix = |channel| {
            let global = maps::sample(pixels, uv, 0, channel, self.blend);
            let local = maps::sample(pixels, local_uv, 1, channel, self.blend);
            global + (local - global) * weight
        };
        WeatherSample::new(
            mix(0),
            mix(1),
            mix(2),
            mix(3) * 32.0,
            mix(8) * 64.0 - 32.0,
            mix(9) * 64.0 - 32.0,
        )
        .with_atmosphere(mix(4), mix(5) * 80.0 - 40.0, mix(6), mix(7))
    }
}
