//! Shared seasonal climate and bounded, replayable surface-cover model.
use glam::DVec3;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimateSample {
    pub temperature_c: f64,
    pub moisture: f64,
    pub seasonality: f64,
}

/// Coarse annual mean by angular latitude; sin(latitude)^2 overcools temperate zones.
pub fn annual_temperature_c(latitude_radians: f64) -> f64 {
    let latitude = latitude_radians / std::f64::consts::FRAC_PI_2;
    28.0 - 52.0 * latitude * latitude
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClimateOffset {
    pub temperature_c: f64,
    pub moisture: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClimateRegion {
    pub center: [f64; 3],
    pub radius_radians: f64,
    pub offset: ClimateOffset,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClimateScenario {
    /// 0 uses Earth's source height; 1 fully applies additional peak exaggeration.
    #[serde(default)]
    pub relief_influence: f64,
    pub start_utc: f64,
    pub end_utc: f64,
    pub global: ClimateOffset,
    pub regions: Vec<ClimateRegion>,
}

impl ClimateScenario {
    pub fn validate(&self) -> Result<(), &'static str> {
        let valid_offset = |o: ClimateOffset| {
            o.temperature_c.is_finite()
                && o.temperature_c.abs() <= 50.0
                && o.moisture.is_finite()
                && o.moisture.abs() <= 1.0
        };
        if !self.start_utc.is_finite()
            || !self.relief_influence.is_finite()
            || !(0.0..=1.0).contains(&self.relief_influence)
            || !self.end_utc.is_finite()
            || self.end_utc < self.start_utc
            || !valid_offset(self.global)
            || self.regions.len() > 32
        {
            return Err("invalid climate scenario");
        }
        for region in &self.regions {
            let center = DVec3::from_array(region.center);
            if !center.is_finite()
                || center.length_squared() < 1e-12
                || !region.radius_radians.is_finite()
                || !(0.0..=std::f64::consts::PI).contains(&region.radius_radians)
                || region.radius_radians == 0.0
                || !valid_offset(region.offset)
            {
                return Err("invalid climate region");
            }
        }
        Ok(())
    }

    pub fn offset(&self, direction: DVec3, utc: f64) -> ClimateOffset {
        let progress = if self.start_utc == self.end_utc {
            f64::from(utc >= self.start_utc)
        } else {
            smooth((utc - self.start_utc) / (self.end_utc - self.start_utc))
        };
        let mut offset = self.global;
        for region in &self.regions {
            let angle = direction
                .dot(DVec3::from_array(region.center).normalize())
                .clamp(-1.0, 1.0)
                .acos();
            let weight = smooth(1.0 - angle / region.radius_radians);
            offset.temperature_c += region.offset.temperature_c * weight;
            offset.moisture += region.offset.moisture * weight;
        }
        offset.temperature_c *= progress;
        offset.moisture *= progress;
        offset
    }
}

/// Opposite seasonal phase on each hemisphere, with a stable tropical-year epoch.
pub fn seasonality(direction: DVec3, utc: f64) -> f64 {
    let year = (utc - 946_728_000.0) / (365.2425 * 86400.0);
    direction.y * (std::f64::consts::TAU * (year - 0.47)).cos()
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SurfaceCoverSample {
    pub vegetation: f32,
    pub snow: f32,
    pub wetness: f32,
}

/// One site evaluation supplies climate and replayed cover to presentation adapters.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceState {
    pub cover: SurfaceCoverSample,
    pub climate: ClimateSample,
    pub source_elevation_m: f64,
    pub climate_elevation_m: f64,
}

/// No incremental state: reconnect and arbitrary time jumps evaluate the same history.
/// Hosts run this on a surface worker, never once per vertex per rendered frame.
pub fn surface_cover(
    utc: f64,
    mut weather: impl FnMut(f64) -> ClimateSample,
) -> SurfaceCoverSample {
    let end_hour = (utc / 3600.0).floor() as i64;
    let mut snow = 0.0_f64;
    let mut wetness = 0.0_f64;
    for hour in (end_hour - 167)..=end_hour {
        let c = weather(hour as f64 * 3600.0);
        let precipitation = ((c.moisture - 0.48) * 2.5).clamp(0.0, 1.0);
        snow = (snow + precipitation * smooth((2.0 - c.temperature_c) / 4.0) * 0.04
            - c.temperature_c.max(0.0) * 0.002)
            .clamp(0.0, 1.0);
        if hour > end_hour - 6 {
            wetness = (wetness + precipitation * smooth((c.temperature_c + 2.0) / 4.0) * 0.45
                - (0.04 + c.temperature_c.max(0.0) * 0.005))
                .clamp(0.0, 1.0);
        }
    }
    let current = weather(end_hour as f64 * 3600.0);
    SurfaceCoverSample {
        vegetation: (smooth((current.temperature_c + 3.0) / 18.0)
            * smooth((current.moisture - 0.08) / 0.5)) as f32,
        snow: snow as f32,
        wetness: wetness as f32,
    }
}

fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
