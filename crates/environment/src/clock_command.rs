//! Time-of-day command values independent of host clocks and I/O.
pub fn parse_hhmm(value: &str) -> Result<u16, String> {
    if value.len() != 4 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("time must have four digits HHMM (0000..2359)".into());
    }
    let value = value.parse::<u16>().map_err(|error| error.to_string())?;
    let (hour, minute) = (value / 100, value % 100);
    if hour > 23 || minute > 59 {
        return Err("time must have four digits HHMM (0000..2359)".into());
    }
    Ok(hour * 60 + minute)
}

/// Keeps the current local mean solar date and sets the requested minute.
/// East longitude advances local time; zero longitude selects UTC.
pub fn utc_at_time(current_ms: i64, minute: u16, longitude_rad: f64) -> Result<i64, String> {
    if minute >= 1440 || !longitude_rad.is_finite() || longitude_rad.abs() > std::f64::consts::PI {
        return Err("invalid time of day or observer longitude".into());
    }
    let offset = (longitude_rad / std::f64::consts::TAU * 86_400_000.0).round() as i64;
    let local_day = (current_ms + offset).div_euclid(86_400_000);
    let target = local_day * 86_400_000 + i64::from(minute) * 60_000 - offset;
    crate::time::WorldTimeMs::new(target).map_err(|error| format!("{error:?}"))?;
    Ok(target)
}
