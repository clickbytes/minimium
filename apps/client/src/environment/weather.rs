//! Local visual weather and optional MET Norway point forecast for Earth.
mod simulated;

use serde_json::Value;
use std::{
    sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant},
};

const REFRESH: Duration = Duration::from_secs(60 * 60);
const RETRY: Duration = Duration::from_secs(10 * 60);
const STALE: Duration = Duration::from_secs(3 * 60 * 60);
const USER_AGENT: &str = "Minimium/0.1 github.com/clickbytes/minimium";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WeatherPreset {
    Clear,
    PartlyCloudy,
    Overcast,
    Rain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WeatherCommand {
    Status,
    Live(bool),
    Simulated,
    Preset(WeatherPreset),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct WeatherSample {
    pub(crate) low: f32,
    pub(crate) middle: f32,
    pub(crate) high: f32,
    pub(crate) rain_mm_h: f32,
    pub(crate) wind_east_m_s: f32,
    pub(crate) wind_north_m_s: f32,
}

impl WeatherPreset {
    pub(crate) fn sample(self) -> WeatherSample {
        match self {
            Self::Clear => WeatherSample::new(0.03, 0.01, 0.02, 0.0, 1.0, 0.0),
            Self::PartlyCloudy => WeatherSample::new(0.35, 0.20, 0.12, 0.0, 2.0, 0.5),
            Self::Overcast => WeatherSample::new(0.92, 0.78, 0.55, 0.0, 4.0, 1.0),
            Self::Rain => WeatherSample::new(0.98, 0.86, 0.58, 3.0, 5.0, 1.5),
        }
    }
}

impl WeatherSample {
    const fn new(
        low: f32,
        middle: f32,
        high: f32,
        rain_mm_h: f32,
        wind_east_m_s: f32,
        wind_north_m_s: f32,
    ) -> Self {
        Self {
            low,
            middle,
            high,
            rain_mm_h,
            wind_east_m_s,
            wind_north_m_s,
        }
    }
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
    preset: WeatherPreset,
    live: bool,
    simulated: bool,
    simulation: simulated::SimulatedWeather,
    last_simulated_sample: Option<WeatherSample>,
    atlas_revision: u64,
    local_atlas: Vec<u8>,
    forecast: Option<Forecast>,
    current_location: Option<Location>,
    pending: Option<PendingForecast>,
    next_request: Instant,
    last_request: Option<(Location, Instant)>,
    last_error: Option<String>,
}

impl WeatherRuntime {
    pub(crate) fn new(simulated: bool) -> Self {
        let preset = WeatherPreset::PartlyCloudy;
        let mut local_atlas = vec![0; simulated::WIDTH * simulated::HEIGHT * 4];
        fill_local_atlas(&mut local_atlas, preset.sample());
        Self {
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
        }
    }

    pub(crate) fn command(&mut self, command: WeatherCommand) -> String {
        match command {
            WeatherCommand::Status => return self.status(),
            WeatherCommand::Live(enabled) => {
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
    ) -> WeatherSample {
        self.receive_pending();
        if (self.simulated || self.live)
            && earth_location.is_some()
            && world_utc_ms.is_some_and(|time| self.simulation.update(time))
        {
            self.atlas_revision += 1;
        }
        if self.simulated {
            self.last_simulated_sample = match (earth_location, world_utc_ms) {
                (Some((lat, lon)), Some(_)) => Some(self.simulation.sample(lat, lon)),
                _ => None,
            };
            return self
                .last_simulated_sample
                .unwrap_or_else(|| self.preset.sample());
        }
        if !self.live {
            return self.preset.sample();
        }
        let Some(location) = earth_location.and_then(|(lat, lon)| Location::new(lat, lon)) else {
            self.current_location = None;
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
        valid.map_or_else(|| self.preset.sample(), |(sample, _)| sample)
    }

    pub(crate) fn status(&self) -> String {
        if self.simulated {
            return self.last_simulated_sample.map_or_else(
                || "weather simulated Earth | waiting for world clock / Earth position".to_owned(),
                |sample| format!(
                    "weather simulated Earth | {:.0}% low, {:.0}% mid, {:.0}% high | rain {:.1} mm/h | no network",
                    sample.low * 100.0,
                    sample.middle * 100.0,
                    sample.high * 100.0,
                    sample.rain_mm_h
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
                    "weather MET Norway forecast {} | {:.0}% low, {:.0}% mid, {:.0}% high | rain {:.1} mm/h",
                    forecast.valid_at,
                    forecast.sample.low * 100.0,
                    forecast.sample.middle * 100.0,
                    forecast.sample.high * 100.0,
                    forecast.sample.rain_mm_h
                );
            }
        }
        if let Some(error) = &self.last_error {
            return format!("weather live unavailable ({error}); local fallback");
        }
        "weather live waiting for Earth position / MET Norway forecast".to_owned()
    }

    pub(crate) fn atlas(&self) -> (&[u8], u64) {
        let pixels = if self.simulated || self.live {
            self.simulation.pixels()
        } else {
            &self.local_atlas
        };
        (pixels, self.atlas_revision)
    }

    fn request(&mut self, location: Location) {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(fetch_forecast(location));
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

fn fill_local_atlas(pixels: &mut [u8], sample: WeatherSample) {
    let color = [
        (sample.low * 255.0) as u8,
        (sample.middle * 255.0) as u8,
        (sample.high * 255.0) as u8,
        ((sample.rain_mm_h / 8.0).min(1.0) * 255.0) as u8,
    ];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&color);
    }
}

fn fetch_forecast(location: Location) -> Result<(WeatherSample, String), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .into();
    let mut response = agent
        .get(&location.url())
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| error.to_string())?;
    let document: Value = response
        .body_mut()
        .read_json()
        .map_err(|error| error.to_string())?;
    let forecast = document["properties"]["timeseries"]
        .as_array()
        .and_then(|series| series.first())
        .ok_or("empty MET Norway forecast")?;
    let details = &forecast["data"]["instant"]["details"];
    let percent = |name: &str| -> Result<f32, String> {
        let value = details[name]
            .as_f64()
            .ok_or_else(|| format!("missing MET Norway {name}"))?;
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err(format!("invalid MET Norway {name}"));
        }
        Ok(value as f32 / 100.0)
    };
    let low = percent("cloud_area_fraction_low")?;
    let middle = percent("cloud_area_fraction_medium")?;
    let high = percent("cloud_area_fraction_high")?;
    let rain = forecast["data"]["next_1_hours"]["details"]["precipitation_amount"]
        .as_f64()
        .unwrap_or(0.0);
    let speed = details["wind_speed"].as_f64().unwrap_or(0.0);
    let direction = details["wind_from_direction"].as_f64().unwrap_or(0.0);
    if !rain.is_finite()
        || !(0.0..=200.0).contains(&rain)
        || !speed.is_finite()
        || !(0.0..=150.0).contains(&speed)
        || !direction.is_finite()
    {
        return Err("invalid MET Norway precipitation or wind".to_owned());
    }
    let from = direction.to_radians();
    let sample = WeatherSample::new(
        low,
        middle,
        high,
        rain as f32,
        (-speed * from.sin()) as f32,
        (-speed * from.cos()) as f32,
    );
    let time = forecast["time"]
        .as_str()
        .ok_or("missing MET Norway forecast time")?;
    Ok((sample, time.to_owned()))
}
