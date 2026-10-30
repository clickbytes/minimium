use super::WeatherSample;

pub(super) const WIDTH: usize = 256;
pub(super) const HEIGHT: usize = 128;
const STEP_MS: i64 = 10 * 60 * 1000;

/// A compact, deterministic world field. The same cells feed the local view and globe.
pub(super) struct SimulatedWeather {
    epoch: Option<i64>,
    pixels: Vec<u8>,
}

impl SimulatedWeather {
    pub(super) fn new() -> Self {
        Self {
            epoch: None,
            pixels: vec![0; WIDTH * HEIGHT * 4],
        }
    }

    pub(super) fn update(&mut self, utc_ms: i64) -> bool {
        let epoch = utc_ms.div_euclid(STEP_MS);
        if self.epoch == Some(epoch) {
            return false;
        }
        self.epoch = Some(epoch);
        let days = (epoch as f64 * STEP_MS as f64) / 86_400_000.0;
        for y in 0..HEIGHT {
            let latitude = (0.5 - (y as f64 + 0.5) / HEIGHT as f64) * std::f64::consts::PI;
            for x in 0..WIDTH {
                let longitude = ((x as f64 + 0.5) / WIDTH as f64 - 0.5) * std::f64::consts::TAU;
                let [low, middle, high, rain] = field(latitude, longitude, days);
                let index = (y * WIDTH + x) * 4;
                self.pixels[index] = (low * 255.0).round() as u8;
                self.pixels[index + 1] = (middle * 255.0).round() as u8;
                self.pixels[index + 2] = (high * 255.0).round() as u8;
                self.pixels[index + 3] = ((rain / 8.0).min(1.0) * 255.0).round() as u8;
            }
        }
        true
    }

    pub(super) fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub(super) fn sample(&self, latitude_deg: f64, longitude_deg: f64) -> WeatherSample {
        let u = ((longitude_deg + 180.0) / 360.0).rem_euclid(1.0);
        let v = ((90.0 - latitude_deg) / 180.0).clamp(0.0, 1.0);
        let x = (u * WIDTH as f64).floor() as usize % WIDTH;
        let y = (v * HEIGHT as f64).floor().min((HEIGHT - 1) as f64) as usize;
        let index = (y * WIDTH + x) * 4;
        let days = self.epoch.unwrap_or(0) as f64 * STEP_MS as f64 / 86_400_000.0;
        let latitude = latitude_deg.to_radians();
        let longitude = longitude_deg.to_radians();
        WeatherSample::new(
            self.pixels[index] as f32 / 255.0,
            self.pixels[index + 1] as f32 / 255.0,
            self.pixels[index + 2] as f32 / 255.0,
            self.pixels[index + 3] as f32 / 255.0 * 8.0,
            (2.0 + latitude.cos().abs() * 5.0) as f32
                * if latitude.abs() > 0.55 { 1.0 } else { -1.0 },
            (longitude * 3.0 + days * 0.35).sin() as f32 * 1.8,
        )
    }
}

fn field(latitude: f64, longitude: f64, days: f64) -> [f64; 4] {
    let front = (longitude * 3.0 + latitude.sin() * 2.3 + days * 0.22).sin() * 0.25
        + (longitude * 7.0 - latitude * 3.0 - days * 0.39).sin() * 0.17
        + (longitude * 13.0 + latitude * 5.0 + days * 0.67).sin() * 0.08;
    let moisture_belt = (-(latitude / 0.27).powi(2)).exp() * 0.17
        + (-((latitude.abs() - 0.9) / 0.28).powi(2)).exp() * 0.16
        - (-((latitude.abs() - 0.48) / 0.18).powi(2)).exp() * 0.12;
    let humidity = (0.46 + front + moisture_belt).clamp(0.0, 1.0);
    let low = (humidity * 1.12 - 0.08).clamp(0.02, 0.98);
    let middle = (humidity * 0.78 + (longitude * 5.0 + days * 0.15).sin() * 0.12).clamp(0.02, 0.95);
    let high =
        (0.16 + humidity * 0.43 + (longitude * 9.0 - days * 0.31).sin() * 0.14).clamp(0.01, 0.9);
    let rain = ((low - 0.55).max(0.0) * (middle - 0.30).max(0.0) * 42.0).clamp(0.0, 8.0);
    [low, middle, high, rain]
}
