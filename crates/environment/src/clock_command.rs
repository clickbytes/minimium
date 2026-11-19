//! Time-of-day command values independent of host clocks and I/O.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockWrite {
    Mode(crate::time::ClockMode),
    Scale(crate::time::TimeScalePpm),
    Offline(crate::time::OfflinePolicy),
    Set(crate::time::WorldTimeMs),
    HourMinute { minute: u16, solar: bool },
}

impl ClockWrite {
    pub fn payload(self) -> minimium_commands::admin::AdminPayload {
        use minimium_commands::{
            admin::AdminPayload,
            operation::{CommandValue as V, OperationId},
            syntax::canonical::AccessKind,
        };
        let (path, access, v) = match self {
            Self::Scale(scale) => (
                "world.time.scale",
                AccessKind::Write,
                V::decimal(i128::from(scale.get()), 6),
            ),
            Self::Mode(mode) => (
                "world.time.mode",
                AccessKind::Write,
                V::Enum(
                    match mode {
                        crate::time::ClockMode::RealTime => "real_time",
                        crate::time::ClockMode::TimeScale => "time_scale",
                    }
                    .into(),
                ),
            ),
            Self::Offline(policy) => (
                "world.time.offline_policy",
                AccessKind::Write,
                V::Enum(
                    match policy {
                        crate::time::OfflinePolicy::Pause => "pause",
                        crate::time::OfflinePolicy::Advance => "advance",
                    }
                    .into(),
                ),
            ),
            Self::Set(instant) => (
                "world.time.set",
                AccessKind::Invoke,
                V::InstantMs(instant.get()),
            ),
            Self::HourMinute { minute, solar } => (
                if solar {
                    "world.time.set_solar_hhmm"
                } else {
                    "world.time.set_utc_hhmm"
                },
                AccessKind::Invoke,
                V::Enum(format!("{:02}{:02}", minute / 60, minute % 60)),
            ),
        };
        let (arguments, value) = if access == AccessKind::Write {
            (Vec::new(), Some(v))
        } else {
            (vec![v], None)
        };
        AdminPayload::Operation {
            operation_id: OperationId(path.into()),
            access,
            arguments,
            value,
        }
    }
}

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

/// East longitude in the Earth-fixed frame. Coordinates are in metres.
pub fn solar_longitude(earth: bool, [x, y, z]: [f64; 3]) -> Result<f64, &'static str> {
    if !earth || ![x, y, z].iter().all(|v| v.is_finite()) || x.hypot(z) < 1.0e-6 {
        return Err("local solar time requires an Earth observer away from the poles");
    }
    Ok(z.atan2(x))
}
