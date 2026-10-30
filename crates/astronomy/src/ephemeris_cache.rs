use std::f64::consts::TAU;

use minimium_core::time::WorldTimeMs;

use crate::{
    ffi,
    frames::{normalize, validate_vector},
    model::{AstronomyContextKey, SkyError, SkySampleContext},
    time::ut_days,
};

const CACHE_STEP_MS: i64 = 300_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CacheKey {
    clock_revision: minimium_core::time::ClockRevision,
    world_context: AstronomyContextKey,
    observer_model_revision: u32,
}

#[derive(Clone, Copy, Debug)]
struct Endpoint {
    utc_ms: i64,
    values: EphemerisValues,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct EphemerisValues {
    pub sun_geocentric_m: [f64; 3],
    pub moon_geocentric_m: [f64; 3],
    pub earth_heliocentric_m: [f64; 3],
    pub moon_north_eqj: [f64; 3],
    pub moon_v_magnitude: f64,
    pub ecliptic_phase_rad: f64,
    pub tt_days: f64,
}

impl EphemerisValues {
    fn from_bridge(raw: ffi::MinimiumAeEphemeris) -> Result<Self, SkyError> {
        let sun_geocentric_m = validate_vector(raw.sun_geocentric_m)?;
        let moon_geocentric_m = validate_vector(raw.moon_geocentric_m)?;
        let earth_heliocentric_m = validate_vector(raw.earth_heliocentric_m)?;
        let moon_north_eqj = normalize(raw.moon_north_eqj)?;
        if !raw.moon_v_magnitude.is_finite()
            || !(-100.0..=100.0).contains(&raw.moon_v_magnitude)
            || !raw.ecliptic_phase_degrees.is_finite()
            || !(0.0..=360.0).contains(&raw.ecliptic_phase_degrees)
            || !raw.tt_days.is_finite()
        {
            return Err(SkyError::InvalidUpstreamValue);
        }
        let ecliptic_phase_rad = raw.ecliptic_phase_degrees.to_radians().rem_euclid(TAU);
        Ok(Self {
            sun_geocentric_m,
            moon_geocentric_m,
            earth_heliocentric_m,
            moon_north_eqj,
            moon_v_magnitude: raw.moon_v_magnitude,
            ecliptic_phase_rad,
            tt_days: raw.tt_days,
        })
    }
}

#[derive(Default)]
pub(crate) struct EphemerisCache {
    key: Option<CacheKey>,
    endpoints: Option<[Endpoint; 2]>,
    evaluations: u64,
    cache_hits: u64,
}

impl EphemerisCache {
    pub fn invalidate(&mut self) {
        self.key = None;
        self.endpoints = None;
    }

    pub fn diagnostics(&self) -> (u64, u64) {
        (self.evaluations, self.cache_hits)
    }

    pub fn sample(
        &mut self,
        context: SkySampleContext,
        force_invalidation: bool,
    ) -> Result<EphemerisValues, SkyError> {
        let key = CacheKey {
            clock_revision: context.clock_revision,
            world_context: context.world_context_key,
            observer_model_revision: context.observer_model_revision,
        };
        if force_invalidation || self.key != Some(key) {
            self.endpoints = None;
        }
        self.key = Some(key);

        let current = context.utc_ms.get();
        let (lower_utc_ms, upper_utc_ms) = bracket(current)?;
        let previous = self.endpoints;
        let lower = previous.and_then(|pair| {
            pair.iter()
                .copied()
                .find(|point| point.utc_ms == lower_utc_ms)
        });
        let upper = previous.and_then(|pair| {
            pair.iter()
                .copied()
                .find(|point| point.utc_ms == upper_utc_ms)
        });
        if lower.is_some() && upper.is_some() {
            self.cache_hits = self.cache_hits.saturating_add(1);
        }
        let lower = match lower {
            Some(point) => point,
            None => {
                self.evaluations = self.evaluations.saturating_add(1);
                compute_endpoint(lower_utc_ms)?
            }
        };
        let upper = match upper {
            Some(point) => point,
            None => {
                self.evaluations = self.evaluations.saturating_add(1);
                compute_endpoint(upper_utc_ms)?
            }
        };
        self.endpoints = Some([lower, upper]);
        interpolate(lower, upper, current)
    }
}

fn bracket(utc_ms: i64) -> Result<(i64, i64), SkyError> {
    let minimum = WorldTimeMs::MIN.get();
    let maximum = WorldTimeMs::MAX_EXCLUSIVE - 1;
    let lower = (utc_ms.div_euclid(CACHE_STEP_MS) * CACHE_STEP_MS).max(minimum);
    let upper = lower
        .checked_add(CACHE_STEP_MS)
        .ok_or(SkyError::InvalidTime)?
        .min(maximum);
    if lower > utc_ms || upper < utc_ms || upper <= lower {
        return Err(SkyError::InvalidTime);
    }
    Ok((lower, upper))
}

fn compute_endpoint(utc_ms: i64) -> Result<Endpoint, SkyError> {
    let time = WorldTimeMs::new(utc_ms).map_err(|_| SkyError::InvalidTime)?;
    let raw = ffi::ephemeris(ut_days(time)?).map_err(map_status)?;
    Ok(Endpoint {
        utc_ms,
        values: EphemerisValues::from_bridge(raw)?,
    })
}

fn interpolate(lower: Endpoint, upper: Endpoint, utc_ms: i64) -> Result<EphemerisValues, SkyError> {
    if lower.utc_ms == upper.utc_ms {
        return Ok(lower.values);
    }
    let alpha = (utc_ms - lower.utc_ms) as f64 / (upper.utc_ms - lower.utc_ms) as f64;
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(SkyError::InvalidTime);
    }
    let a = lower.values;
    let b = upper.values;
    let phase_x = a.ecliptic_phase_rad.cos() * (1.0 - alpha) + b.ecliptic_phase_rad.cos() * alpha;
    let phase_y = a.ecliptic_phase_rad.sin() * (1.0 - alpha) + b.ecliptic_phase_rad.sin() * alpha;
    if phase_x.hypot(phase_y) <= f64::EPSILON {
        return Err(SkyError::DegenerateVector);
    }
    let ecliptic_phase_rad = phase_y.atan2(phase_x).rem_euclid(TAU);
    Ok(EphemerisValues {
        sun_geocentric_m: lerp3(a.sun_geocentric_m, b.sun_geocentric_m, alpha),
        moon_geocentric_m: lerp3(a.moon_geocentric_m, b.moon_geocentric_m, alpha),
        earth_heliocentric_m: lerp3(a.earth_heliocentric_m, b.earth_heliocentric_m, alpha),
        moon_north_eqj: normalize(lerp3(a.moon_north_eqj, b.moon_north_eqj, alpha))?,
        moon_v_magnitude: lerp(a.moon_v_magnitude, b.moon_v_magnitude, alpha),
        ecliptic_phase_rad,
        tt_days: lerp(a.tt_days, b.tt_days, alpha),
    })
}

fn lerp(a: f64, b: f64, alpha: f64) -> f64 {
    a + (b - a) * alpha
}

fn lerp3(a: [f64; 3], b: [f64; 3], alpha: f64) -> [f64; 3] {
    std::array::from_fn(|index| lerp(a[index], b[index], alpha))
}

pub(crate) fn map_status(status: i32) -> SkyError {
    match status {
        -1 => SkyError::InvalidTime,
        -2 => SkyError::NonFinite,
        -3 => SkyError::InvalidObserver,
        other => SkyError::UpstreamStatus(other),
    }
}
