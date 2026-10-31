use super::{sampling, CloudFieldSnapshot};
use crate::environment::weather::{lightning, LightningEvent, WeatherSample};
use glam::DVec3;
use std::sync::Arc;

pub(crate) struct PresentedCloudFrame {
    pub(crate) fields: [Arc<CloudFieldSnapshot>; 2],
}

pub(crate) struct PresentedWeather {
    pub(crate) rain_ceiling_m: f32,
    pub(crate) sample: WeatherSample,
    pub(crate) lightning: Option<LightningEvent>,
    pub(crate) blend: f32,
    pub(crate) region: [f32; 4],
}

impl PresentedCloudFrame {
    pub(crate) fn presentation(&self, location: (f64, f64), morph: f32) -> PresentedWeather {
        let [a, b] = &self.fields;
        let seconds = a.seconds + (b.seconds - a.seconds) * f64::from(morph);
        let at = |lat: f64, lon: f64| {
            let (lat, lon) = (lat.to_radians(), lon.to_radians());
            let up = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
            let pairs = sampling::paired(a.lobes(up, 0.0), b.lobes(up, 0.0));
            (
                up,
                sampling::column(
                    pairs
                        .iter()
                        .map(|p| sampling::interpolate(&p[0], &p[1], f64::from(morph))),
                    up,
                ),
            )
        };
        let (up, physical) = at(location.0, location.1);
        let blend = a.blend + (b.blend - a.blend) * morph;
        let column = a.atmosphere.column(up, blend);
        PresentedWeather {
            rain_ceiling_m: physical.top_m[0].max(physical.top_m[1]) as f32,
            sample: WeatherSample {
                low: physical.coverage[0] as f32,
                middle: physical.coverage[1] as f32,
                high: physical.coverage[2] as f32,
                rain_mm_h: physical.rain_mm_h as f32,
                wind_east_m_s: column[2].x * 64.0 - 32.0,
                wind_north_m_s: column[2].y * 64.0 - 32.0,
                humidity: column[1].x,
                temperature_c: column[1].y * 80.0 - 40.0,
                fog: column[1].z,
                ground_moisture: column[1].w,
            },
            lightning: lightning::event(a.seed, (seconds * 1000.0) as i64, location, |lat, lon| {
                let (_, sample) = at(lat, lon);
                (
                    sample.rain_mm_h as f32,
                    sample.storm as f32,
                    sample.storm_id.map(|id| id.hash()),
                )
            }),
            blend,
            region: a.atmosphere.region,
        }
    }
}
