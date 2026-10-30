use super::*;
use minimium_core::time::{
    ClockId, ClockMode, ClockRevision, OfflinePolicy, SavedClockState, TimeScalePpm, WorldTimeMs,
};
use std::{io, io::Write};

pub(super) fn parse_saved_clock(line: &str, source_version: u8) -> io::Result<SavedClockState> {
    let mut fields = line.split('\t');
    if fields.next() != Some("CLOCK") {
        return Err(invalid_world_snapshot("invalid CLOCK record"));
    }
    let clock_id = ClockId::new(parse_decimal(
        fields.next(),
        "missing clock id",
        "invalid clock id",
    )?)
    .map_err(|_| invalid_world_snapshot("unsupported clock id"))?;
    let revision = ClockRevision::new(parse_decimal(
        fields.next(),
        "missing clock revision",
        "invalid clock revision",
    )?)
    .map_err(|_| invalid_world_snapshot("clock revision must be nonzero"))?;
    let mode = ClockMode::try_from(parse_decimal::<u8>(
        fields.next(),
        "missing clock mode",
        "invalid clock mode",
    )?)
    .map_err(|_| invalid_world_snapshot("unsupported clock mode"))?;
    let world_time = WorldTimeMs::new(parse_decimal(
        fields.next(),
        "missing world UTC time",
        "invalid world UTC time",
    )?)
    .map_err(|_| invalid_world_snapshot("world UTC time is out of range"))?;
    let wall_time = WorldTimeMs::new(parse_decimal(
        fields.next(),
        "missing saved wall UTC time",
        "invalid saved wall UTC time",
    )?)
    .map_err(|_| invalid_world_snapshot("saved wall UTC time is out of range"))?;
    let rate = TimeScalePpm::new(parse_decimal(
        fields.next(),
        "missing clock rate",
        "invalid clock rate",
    )?)
    .map_err(|_| invalid_world_snapshot("clock rate is out of range"))?;
    if source_version == 8 && rate.get() > 1_440_000_000 {
        return Err(invalid_world_snapshot("v8 clock rate is out of range"));
    }
    let offline_policy = OfflinePolicy::try_from(parse_decimal::<u8>(
        fields.next(),
        "missing offline policy",
        "invalid offline policy",
    )?)
    .map_err(|_| invalid_world_snapshot("unsupported offline policy"))?;
    if fields.next().is_some() {
        return Err(invalid_world_snapshot("CLOCK record has extra fields"));
    }
    SavedClockState::new(
        clock_id,
        revision,
        mode,
        world_time,
        wall_time,
        rate,
        offline_policy,
    )
    .map_err(|_| invalid_world_snapshot("invalid saved clock state"))
}

pub(super) fn write_saved_clock(output: &mut impl Write, state: SavedClockState) -> io::Result<()> {
    write!(
        output,
        "CLOCK\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        state.clock_id().get(),
        state.revision().get(),
        state.mode() as u8,
        state.saved_world_utc_ms().get(),
        state.saved_wall_utc_ms().get(),
        state.rate_ppm().get(),
        state.offline_policy() as u8,
    )
}

fn parse_decimal<T>(
    value: Option<&str>,
    missing: &'static str,
    invalid: &'static str,
) -> io::Result<T>
where
    T: std::str::FromStr,
{
    let value = value.ok_or_else(|| invalid_world_snapshot(missing))?;
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_world_snapshot(invalid));
    }
    value.parse().map_err(|_| invalid_world_snapshot(invalid))
}
