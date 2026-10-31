//! Local visual weather and optional MET Norway point forecast for Earth.
mod sample;
use sample::fill_local_atlas;
pub(crate) use sample::WeatherSample;
mod bootstrap;
mod climate;
pub(crate) mod clouds;
mod column;
mod dynamics;
mod grid;
mod patterns;
mod systems;
pub(crate) mod transport;
pub(crate) mod wind;
use grid::CHANNELS;
pub(crate) use grid::{WeatherResolution, ATLAS_LAYERS};
pub(crate) use terrain::TerrainConfig;
mod forecast;
pub(crate) mod lakes;
pub(crate) mod lightning;
mod maps;
mod region;
mod shelter;
mod simulated;
mod terrain;
mod thunder;
pub(crate) use lightning::LightningEvent;

use minimium_core::world::{CollisionIndex, SkyCoverQuery};
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};

const REFRESH: Duration = Duration::from_secs(60 * 60);
const RETRY: Duration = Duration::from_secs(10 * 60);
const STALE: Duration = Duration::from_secs(3 * 60 * 60);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WeatherPreset {
    Clear,
    PartlyCloudy,
    Overcast,
    Rain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WeatherCommand {
    Clouds(clouds::settings::CloudCommand),
    Status,
    Live(bool),
    Simulated,
    Preset(WeatherPreset),
    Resolution(Option<WeatherResolution>),
    Globe(Option<bool>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Location {
    latitude_hundredths: i32,
    longitude_hundredths: i32,
}

impl Location {
    fn new(latitude: f64, longitude: f64) -> Option<Self> {
        (latitude.is_finite()
            && longitude.is_finite()
            && (-90.0..=90.0).contains(&latitude)
            && (-180.0..=180.0).contains(&longitude))
        .then_some(Self {
            latitude_hundredths: (latitude * 100.0).round() as i32,
            longitude_hundredths: (longitude * 100.0).round() as i32,
        })
    }

    fn url(self) -> String {
        format!(
            "https://api.met.no/weatherapi/locationforecast/2.0/compact?lat={:.2}&lon={:.2}",
            self.latitude_hundredths as f64 / 100.0,
            self.longitude_hundredths as f64 / 100.0
        )
    }
}

struct Forecast {
    location: Location,
    sample: WeatherSample,
    valid_at: String,
    received_at: Instant,
}

type ForecastResult = Result<(WeatherSample, String), String>;
type PendingForecast = (Location, Receiver<ForecastResult>);

pub(crate) struct WeatherRuntime {
    pub(crate) readable_globe: bool,
    preset: WeatherPreset,
    live: bool,
    simulated: bool,
    simulation: simulated::SimulatedWeather,
    last_simulated_sample: Option<WeatherSample>,
    atlas_revision: u64,
    local_atlas: [Vec<u8>; CHANNELS / 4],
    forecast: Option<Forecast>,
    current_location: Option<Location>,
    pending: Option<PendingForecast>,
    next_request: Instant,
    last_request: Option<(Location, Instant)>,
    last_error: Option<String>,
    clock_rate_ppm: Option<u64>,
    world_utc_ms: Option<i64>,
    earth_available: bool,
    notice: Option<String>,
    lightning: Option<LightningEvent>,
    cover_query: SkyCoverQuery,
    shelter_mask: [u16; 4],
    shelter_eye: [f32; 3],
    shelter_updated: Option<Instant>,
    last_thunder_id: Option<u64>,
}

impl WeatherRuntime {
    pub(crate) fn new(simulated: bool) -> Self {
        let preset = WeatherPreset::PartlyCloudy;
        let mut local_atlas =
            std::array::from_fn(|_| vec![0; simulated::WIDTH * simulated::HEIGHT * 4]);
        fill_local_atlas(&mut local_atlas, preset.sample());
        Self {
            readable_globe: true,
            preset,
            live: false,
            simulated,
            simulation: simulated::SimulatedWeather::new(),
            last_simulated_sample: None,
            atlas_revision: 1,
            local_atlas,
            forecast: None,
            current_location: None,
            pending: None,
            next_request: Instant::now(),
            last_request: None,
            last_error: None,
            clock_rate_ppm: None,
            world_utc_ms: None,
            earth_available: false,
            notice: None,
            lightning: None,
            cover_query: SkyCoverQuery::default(),
            shelter_mask: [0; 4],
            shelter_eye: [f32::INFINITY; 3],
            shelter_updated: None,
            last_thunder_id: None,
        }
    }

    pub(crate) fn command(&mut self, command: WeatherCommand) -> String {
        match command {
            WeatherCommand::Clouds(_) => {
                return "cloud settings are handled by the renderer".to_owned()
            }
            WeatherCommand::Globe(readable) => {
                if let Some(readable) = readable {
                    self.readable_globe = readable;
                }
                return format!(
                    "weather globe {} (presentation only)",
                    if self.readable_globe {
                        "readable"
                    } else {
                        "natural"
                    }
                );
            }
            WeatherCommand::Status => return self.status(),
            WeatherCommand::Resolution(resolution) => {
                if let Some(resolution) = resolution {
                    self.simulation.set_resolution(resolution);
                }
                return self.simulation.resolution().description();
            }
            WeatherCommand::Live(enabled) => {
                if enabled && (!self.earth_available || !self.live_clock_eligible()) {
                    return "weather live requires Earth at 1x world time within 5 minutes of UTC"
                        .to_owned();
                }
                self.live = enabled;
                self.simulated = !enabled;
                self.next_request = Instant::now();
                self.atlas_revision += 1;
            }
            WeatherCommand::Simulated => {
                self.live = false;
                self.simulated = true;
                self.atlas_revision += 1;
            }
            WeatherCommand::Preset(preset) => {
                self.preset = preset;
                self.live = false;
                self.simulated = false;
                fill_local_atlas(&mut self.local_atlas, preset.sample());
                self.atlas_revision += 1;
            }
        }
        self.status()
    }

    pub(crate) fn update(
        &mut self,
        earth_location: Option<(f64, f64)>,
        world_utc_ms: Option<i64>,
        seed: u128,
        rate_ppm: Option<u64>,
        clock_revision: Option<u64>,
        terrain: Option<TerrainConfig>,
    ) -> WeatherSample {
        if self
            .world_utc_ms
            .zip(world_utc_ms)
            .is_some_and(|(previous, current)| current < previous)
        {
            self.last_thunder_id = None;
        }
        self.world_utc_ms = world_utc_ms;
        self.earth_available = earth_location.is_some();
        self.clock_rate_ppm = rate_ppm;
        if self.live && !self.live_clock_eligible() {
            self.live = false;
            self.simulated = true;
            self.last_error =
                Some("world clock left 1x / current UTC; switched to simulation".to_owned());
            self.notice = self.last_error.clone();
            self.atlas_revision += 1;
        }
        self.receive_pending();
        let correction = if self.live {
            self.forecast.as_ref().and_then(|forecast| {
                (forecast.received_at.elapsed() < STALE
                    && earth_location.and_then(|(lat, lon)| Location::new(lat, lon))
                        == Some(forecast.location))
                .then_some((
                    forecast.location.latitude_hundredths as f64 / 100.0,
                    forecast.location.longitude_hundredths as f64 / 100.0,
                    forecast.sample,
                ))
            })
        } else {
            None
        };
        self.simulation.set_correction(correction);
        if (self.simulated || self.live)
            && world_utc_ms.is_some_and(|time| {
                self.simulation
                    .update(seed, time, clock_revision, terrain, earth_location)
            })
        {
            self.atlas_revision += 1;
        }
        if self.simulated {
            self.last_simulated_sample = match (earth_location, world_utc_ms) {
                (Some((lat, lon)), Some(_)) => Some(self.simulation.sample(lat, lon)),
                _ => None,
            };
            let sample = self
                .last_simulated_sample
                .unwrap_or_else(|| self.preset.sample());
            return sample;
        }
        if !self.live {
            self.lightning = None;
            return self.preset.sample();
        }
        let Some(location) = earth_location.and_then(|(lat, lon)| Location::new(lat, lon)) else {
            self.current_location = None;
            self.lightning = None;
            return self.preset.sample();
        };
        self.current_location = Some(location);
        let valid = self
            .forecast
            .as_ref()
            .filter(|forecast| {
                forecast.location == location && forecast.received_at.elapsed() < STALE
            })
            .map(|forecast| (forecast.sample, forecast.received_at.elapsed()));
        let refresh_due = valid.is_none_or(|(_, age)| age >= REFRESH);
        let changed_location = self
            .last_request
            .is_some_and(|(previous, at)| previous != location && at.elapsed() >= RETRY);
        if refresh_due
            && self.pending.is_none()
            && (Instant::now() >= self.next_request || changed_location)
        {
            self.request(location);
        }
        // Apply a received forecast only when the corrected shared atlas is ready.
        let (latitude, longitude) = earth_location.unwrap();
        self.simulation.sample(latitude, longitude)
    }

    pub(crate) fn status(&self) -> String {
        if self.simulated {
            return self.last_simulated_sample.map_or_else(
                || "weather simulated Earth | waiting for world clock / Earth position".to_owned(),
                |sample| format!(
                    "weather simulated Earth | local column cover {:.0}% low, {:.0}% mid, {:.0}% high | local precipitation {:.1} mm/h | fog {:.0}% | {} | no network",
                    sample.low * 100.0,
                    sample.middle * 100.0,
                    sample.high * 100.0,
                    sample.rain_mm_h,
                    sample.fog * 100.0,
                    self.simulation.status()
                ),
            );
        }
        if !self.live {
            return format!("weather local {:?}", self.preset);
        }
        if let Some(forecast) = &self.forecast {
            if Some(forecast.location) == self.current_location
                && forecast.received_at.elapsed() < STALE
            {
                return format!(
                    "weather MET Norway forecast {} | {:.0}% low, {:.0}% mid, {:.0}% high | precipitation {:.1} mm/h | humidity {:.0}%",
                    forecast.valid_at,
                    forecast.sample.low * 100.0,
                    forecast.sample.middle * 100.0,
                    forecast.sample.high * 100.0,
                    forecast.sample.rain_mm_h,
                    forecast.sample.humidity * 100.0
                );
            }
        }
        if let Some(error) = &self.last_error {
            return format!("weather live unavailable ({error}); simulated fallback");
        }
        "weather live waiting for Earth position / MET Norway forecast".to_owned()
    }

    pub(crate) fn atlas(&self) -> ([&[u8]; ATLAS_LAYERS], u64, f32, [f32; 4], Option<f64>) {
        let pixels = if self.simulated || self.live {
            self.simulation.pixels()
        } else {
            std::array::from_fn(|i| self.local_atlas[(i % grid::SNAPSHOT_LAYERS) / 2].as_slice())
        };
        let region = if self.simulated || self.live {
            self.simulation.region()
        } else {
            [0.0; 4]
        };
        let start_seconds = if self.simulated || self.live {
            self.simulation.start_seconds()
        } else {
            None
        };
        (
            pixels,
            self.atlas_revision,
            self.simulation.blend(),
            region,
            start_seconds,
        )
    }

    pub(crate) fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    pub(crate) fn lightning(&self) -> Option<LightningEvent> {
        self.lightning
    }

    pub(crate) fn present(&mut self, sample: WeatherSample, lightning: Option<LightningEvent>) {
        self.lightning = lightning;
        if self.simulated {
            self.last_simulated_sample = Some(sample);
        }
    }

    pub(crate) fn shelter_mask(
        &mut self,
        index: &CollisionIndex,
        eye: [f32; 3],
        up: [f32; 3],
    ) -> [u16; 4] {
        shelter::mask(self, index, eye, up)
    }

    pub(crate) fn schedule_thunder(&mut self, eye: [f32; 3], radius: f32) {
        let Some(event) = self.lightning else {
            return;
        };
        if self.last_thunder_id == Some(event.id) {
            return;
        }
        self.last_thunder_id = Some(event.id);
        let scale = (radius / 6_371_000.0).max(0.0001);
        let point = event
            .direction
            .map(|value| value * (radius + 4000.0 * scale));
        let distance = point
            .iter()
            .zip(eye)
            .map(|(a, b)| (a - b).powi(2))
            .sum::<f32>()
            .sqrt();
        if distance < 30_000.0 * scale {
            thunder::schedule(event.id, distance / (343.0 * scale));
        }
    }

    fn live_clock_eligible(&self) -> bool {
        self.clock_rate_ppm == Some(1_000_000)
            && self.world_utc_ms.is_some_and(|world| {
                let real = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|duration| duration.as_millis() as i64);
                real.is_some_and(|real| world.abs_diff(real) <= 5 * 60 * 1000)
            })
    }

    fn request(&mut self, location: Location) {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(forecast::fetch(location));
        });
        self.pending = Some((location, receiver));
        self.last_request = Some((location, Instant::now()));
        self.next_request = Instant::now() + RETRY;
    }

    fn receive_pending(&mut self) {
        let Some((location, receiver)) = &self.pending else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err("weather request ended unexpectedly".to_owned()),
        };
        let location = *location;
        self.pending = None;
        match result {
            Ok((sample, valid_at)) => {
                self.forecast = Some(Forecast {
                    location,
                    sample,
                    valid_at,
                    received_at: Instant::now(),
                });
                self.next_request = Instant::now() + REFRESH;
                self.last_error = None;
            }
            Err(error) => self.last_error = Some(error),
        }
    }
}
