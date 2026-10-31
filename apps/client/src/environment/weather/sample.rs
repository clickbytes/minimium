use super::WeatherPreset;
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WeatherSample {
    pub(crate) low: f32,
    pub(crate) middle: f32,
    pub(crate) high: f32,
    pub(crate) rain_mm_h: f32,
    pub(crate) wind_east_m_s: f32,
    pub(crate) wind_north_m_s: f32,
    pub(crate) humidity: f32,
    pub(crate) temperature_c: f32,
    pub(crate) fog: f32,
    pub(crate) ground_moisture: f32,
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
    pub(crate) const fn clear() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0)
    }
    pub(super) const fn new(
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
            humidity: (low * 0.45 + middle * 0.25 + 0.25).clamp(0.0, 1.0),
            temperature_c: 12.0,
            fog: 0.0,
            ground_moisture: (rain_mm_h / 8.0).clamp(0.0, 1.0),
        }
    }

    pub(crate) fn with_atmosphere(
        mut self,
        humidity: f32,
        temperature_c: f32,
        fog: f32,
        ground_moisture: f32,
    ) -> Self {
        self.humidity = humidity;
        self.temperature_c = temperature_c;
        self.fog = fog;
        self.ground_moisture = ground_moisture;
        self
    }
}

pub(super) fn fill_local_atlas(
    pixels: &mut [Vec<u8>; super::grid::CHANNELS / 4],
    sample: WeatherSample,
) {
    let encoded = super::maps::encode(super::column::preset(sample));
    for (pixels, color) in pixels.iter_mut().zip(encoded) {
        for pixel in pixels.as_chunks_mut::<4>().0 {
            pixel.copy_from_slice(&color);
        }
    }
}
