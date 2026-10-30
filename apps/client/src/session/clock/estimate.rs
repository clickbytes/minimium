use super::ClockSample;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EstimateKey {
    revision: u64,
    anchor_monotonic_ms: u64,
    anchor_world_utc_ms: i64,
    rate_ppm: u64,
    mode: u8,
    offline_policy: u8,
    offset_twice_ms: Option<i128>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct OffsetSample {
    /// Twice the offset in milliseconds, preserving half-millisecond results.
    pub(super) offset_twice_ms: i128,
    pub(super) rtt_ms: u64,
    pub(super) received_at_ms: u64,
}

pub(super) fn estimate_key(sample: ClockSample, offset: Option<OffsetSample>) -> EstimateKey {
    EstimateKey {
        revision: sample.revision().get(),
        anchor_monotonic_ms: sample.server_monotonic_ms().get(),
        anchor_world_utc_ms: sample.world_utc_ms().get(),
        rate_ppm: sample.rate_ppm().get(),
        mode: sample.mode() as u8,
        offline_policy: sample.offline_policy() as u8,
        offset_twice_ms: offset.map(|sample| sample.offset_twice_ms),
    }
}

/// Welcome and environment samples provide a bounded, latency-uncorrected anchor.
pub(super) fn estimate_welcome_time(
    sample: ClockSample,
    local_monotonic_ms: u64,
    sample_received_at_ms: u64,
) -> Option<f64> {
    let elapsed_ms = i128::from(local_monotonic_ms.saturating_sub(sample_received_at_ms));
    let scaled = elapsed_ms.checked_mul(i128::from(sample.rate_ppm().get()))?;
    let integer_delta = scaled.div_euclid(1_000_000);
    let remainder = scaled.rem_euclid(1_000_000);
    let world_integer = i128::from(sample.world_utc_ms().get()).checked_add(integer_delta)?;
    if world_integer < i128::from(minimium_core::time::MIN_WORLD_TIME_MS)
        || world_integer >= i128::from(minimium_core::time::MAX_WORLD_TIME_MS_EXCLUSIVE)
    {
        return None;
    }
    Some(world_integer as f64 + remainder as f64 / 1_000_000.0)
}

pub(super) fn estimate_world_time(
    sample: ClockSample,
    local_monotonic_ms: u64,
    offset: OffsetSample,
) -> Option<(f64, bool)> {
    let estimated_server_twice = i128::from(local_monotonic_ms)
        .checked_mul(2)?
        .checked_add(offset.offset_twice_ms)?;
    let anchor_twice = i128::from(sample.server_monotonic_ms().get()).checked_mul(2)?;
    let elapsed_twice = estimated_server_twice.checked_sub(anchor_twice)?;
    let before_anchor = elapsed_twice < 0;
    let elapsed_twice = elapsed_twice.max(0);
    let world_delta_numerator = elapsed_twice.checked_mul(i128::from(sample.rate_ppm().get()))?;
    let world_delta_integer_ms = world_delta_numerator.div_euclid(2_000_000);
    let world_remainder = world_delta_numerator.rem_euclid(2_000_000);
    let world_ms_integer =
        i128::from(sample.world_utc_ms().get()).checked_add(world_delta_integer_ms)?;
    if world_ms_integer < i128::from(minimium_core::time::MIN_WORLD_TIME_MS)
        || world_ms_integer >= i128::from(minimium_core::time::MAX_WORLD_TIME_MS_EXCLUSIVE)
    {
        return None;
    }
    let world_ms = world_ms_integer as f64 + world_remainder as f64 / 2_000_000.0;
    Some((world_ms, before_anchor))
}

pub(super) fn rounded_time(world_ms: f64) -> Option<i64> {
    let rounded = world_ms.round();
    if !rounded.is_finite()
        || rounded < minimium_core::time::MIN_WORLD_TIME_MS as f64
        || rounded >= minimium_core::time::MAX_WORLD_TIME_MS_EXCLUSIVE as f64
    {
        None
    } else {
        Some(rounded as i64)
    }
}
