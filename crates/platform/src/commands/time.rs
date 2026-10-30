use minimium_core::time::{ClockMode, OfflinePolicy, TimeScalePpm, WorldTimeMs};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeCommand {
    Status,
    Mode(ClockMode),
    Set(WorldTimeMs),
    Scale(TimeScalePpm),
    Offline(OfflinePolicy),
}

impl TimeCommand {
    pub(super) fn parse(arguments: &[&str]) -> Result<Self, String> {
        match arguments {
            ["status"] => Ok(Self::Status),
            ["mode", "real_time"] => Ok(Self::Mode(ClockMode::RealTime)),
            ["mode", "time_scale"] => Ok(Self::Mode(ClockMode::TimeScale)),
            ["set", value] => parse_world_time_utc(value).map(Self::Set),
            ["scale", value] => parse_scale_ppm(value).map(Self::Scale),
            ["offline", "pause"] => Ok(Self::Offline(OfflinePolicy::Pause)),
            ["offline", "advance"] => Ok(Self::Offline(OfflinePolicy::Advance)),
            _ => Err(String::from(TIME_USAGE)),
        }
    }
}

pub const TIME_USAGE: &str =
    "time status | time mode real_time|time_scale | time set RFC3339_UTC | time scale DECIMAL | time offline pause|advance";

/// Parses an RFC3339 UTC value with a `Z` or `+00:00` offset and up to ms precision.
pub fn parse_world_time_utc(value: &str) -> Result<WorldTimeMs, String> {
    let bytes = value.as_bytes();
    let zone_start = if value.ends_with('Z') {
        bytes.len().checked_sub(1)
    } else if value.ends_with("+00:00") {
        bytes.len().checked_sub(6)
    } else {
        None
    }
    .ok_or_else(invalid_utc)?;
    let local = &bytes[..zone_start];
    let (date_time, fraction) = match local.iter().position(|byte| *byte == b'.') {
        Some(separator) => {
            let fraction = &local[separator + 1..];
            if fraction.is_empty() || fraction.len() > 3 || !digits(fraction) {
                return Err(invalid_utc());
            }
            (&local[..separator], fraction)
        }
        None => (local, &[][..]),
    };
    if date_time.len() != 19
        || date_time[4] != b'-'
        || date_time[7] != b'-'
        || date_time[10] != b'T'
        || date_time[13] != b':'
        || date_time[16] != b':'
    {
        return Err(invalid_utc());
    }
    let year = field(date_time, 0, 4).ok_or_else(invalid_utc)? as i64;
    let month = field(date_time, 5, 7).ok_or_else(invalid_utc)?;
    let day = field(date_time, 8, 10).ok_or_else(invalid_utc)?;
    let hour = field(date_time, 11, 13).ok_or_else(invalid_utc)?;
    let minute = field(date_time, 14, 16).ok_or_else(invalid_utc)?;
    let second = field(date_time, 17, 19).ok_or_else(invalid_utc)?;
    if year == 0
        || !(1..=12).contains(&month)
        || day == 0
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err(invalid_utc());
    }
    let millis = match fraction.len() {
        0 => 0,
        1 => u32::from(fraction[0] - b'0') * 100,
        2 => u32::from(fraction[0] - b'0') * 100 + u32::from(fraction[1] - b'0') * 10,
        3 => field(fraction, 0, 3).ok_or_else(invalid_utc)?,
        _ => return Err(invalid_utc()),
    };
    let days = days_from_civil(year, month, day);
    let time_ms = i128::from(hour) * 3_600_000
        + i128::from(minute) * 60_000
        + i128::from(second) * 1_000
        + i128::from(millis);
    let timestamp = i128::from(days)
        .checked_mul(86_400_000)
        .and_then(|value| value.checked_add(time_ms))
        .ok_or_else(invalid_utc)?;
    let timestamp = i64::try_from(timestamp).map_err(|_| invalid_utc())?;
    WorldTimeMs::new(timestamp).map_err(|_| {
        String::from("UTC date must be in [2000-01-01T00:00:00Z, 2101-01-01T00:00:00Z)")
    })
}

/// Parses a decimal rate without floating point or silent rounding.
pub fn parse_scale_ppm(value: &str) -> Result<TimeScalePpm, String> {
    let mut parts = value.split('.');
    let whole_text = parts.next().unwrap_or_default();
    let fraction_text = parts.next();
    if parts.next().is_some()
        || whole_text.is_empty()
        || !whole_text.bytes().all(|byte| byte.is_ascii_digit())
        || fraction_text.is_some_and(|fraction| {
            fraction.is_empty()
                || fraction.len() > 6
                || !fraction.bytes().all(|byte| byte.is_ascii_digit())
        })
    {
        return Err(String::from(
            "rate must be 0 (pause) or a decimal from 0.1 to 1000000 with at most 6 fractional digits",
        ));
    }
    let whole = whole_text.parse::<u64>().map_err(|_| invalid_scale())?;
    let mut fraction = fraction_text.unwrap_or_default().to_owned();
    while fraction.len() < 6 {
        fraction.push('0');
    }
    let fractional_ppm = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<u64>().map_err(|_| invalid_scale())?
    };
    let ppm = whole
        .checked_mul(1_000_000)
        .and_then(|value| value.checked_add(fractional_ppm))
        .ok_or_else(invalid_scale)?;
    if ppm != 0 && ppm < 100_000 {
        return Err(invalid_scale());
    }
    TimeScalePpm::new(ppm).map_err(|_| invalid_scale())
}

fn field(bytes: &[u8], start: usize, end: usize) -> Option<u32> {
    let value = bytes.get(start..end)?;
    if value.is_empty() || !digits(value) {
        return None;
    }
    value.iter().try_fold(0_u32, |number, digit| {
        number
            .checked_mul(10)?
            .checked_add(u32::from(*digit - b'0'))
    })
}

fn digits(bytes: &[u8]) -> bool {
    bytes.iter().all(u8::is_ascii_digit)
}

fn leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let shifted_month = i64::from(month) + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn invalid_utc() -> String {
    String::from(
        "date must be RFC3339 UTC (Z or +00:00), with 0–3 millisecond digits and no leap second",
    )
}

fn invalid_scale() -> String {
    String::from(
        "rate must be 0 (pause) or between 0.1 and 1000000 with at most 6 fractional digits",
    )
}
