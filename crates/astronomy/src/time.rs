use minimium_core::time::{WorldTimeMs, MAX_WORLD_TIME_MS_EXCLUSIVE, MIN_WORLD_TIME_MS};

use crate::model::SkyError;

pub(crate) const J2000_NOON_UTC_MS: i64 = 946_728_000_000;
pub(crate) const MILLIS_PER_DAY: i64 = 86_400_000;
pub(crate) const JULIAN_YEAR_DAYS: f64 = 365.25;

pub(crate) fn ut_days(utc_ms: WorldTimeMs) -> Result<f64, SkyError> {
    let value = utc_ms.get();
    if !(MIN_WORLD_TIME_MS..MAX_WORLD_TIME_MS_EXCLUSIVE).contains(&value) {
        return Err(SkyError::InvalidTime);
    }
    let delta_ms = value
        .checked_sub(J2000_NOON_UTC_MS)
        .ok_or(SkyError::InvalidTime)?;
    let days = delta_ms as f64 / MILLIS_PER_DAY as f64;
    if days.is_finite() {
        Ok(days)
    } else {
        Err(SkyError::InvalidTime)
    }
}
