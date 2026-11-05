use super::types::{
    ClockAnchor, ClockMode, ClockSample, MonotonicMs, OfflinePolicy, SavedClockState, SessionNonce,
    TimeError, TimeScalePpm, WorldTimeMs, MAX_WORLD_TIME_MS_EXCLUSIVE, MIN_WORLD_TIME_MS,
};

const SCALE_DENOMINATOR: i128 = 1_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RestoredClock {
    anchor: ClockAnchor,
    wall_clock_went_backwards: bool,
}

impl RestoredClock {
    pub const fn anchor(self) -> ClockAnchor {
        self.anchor
    }

    pub const fn wall_clock_went_backwards(self) -> bool {
        self.wall_clock_went_backwards
    }
}

/// Evaluate one UTC value from an immutable anchor and a session-relative timestamp.
pub fn evaluate_clock(anchor: ClockAnchor, now: MonotonicMs) -> Result<WorldTimeMs, TimeError> {
    let elapsed_ms = monotonic_delta(anchor.monotonic_ms(), now)?;
    let rate_ppm = effective_rate(anchor.mode(), anchor.rate_ppm());
    let delta_world_ms = scaled_elapsed(elapsed_ms, rate_ppm)?;
    let result = i128::from(anchor.world_utc_ms().get())
        .checked_add(delta_world_ms)
        .ok_or(TimeError::ArithmeticOverflow)?;
    world_time_from_i128(result)
}

/// Build a sample whose UTC is evaluated for exactly its monotonic timestamp.
pub fn sample_clock(
    anchor: ClockAnchor,
    session_nonce: SessionNonce,
    at: MonotonicMs,
) -> Result<ClockSample, TimeError> {
    ClockSample::new(
        anchor.clock_id(),
        anchor.revision(),
        session_nonce,
        at,
        evaluate_clock(anchor, at)?,
        anchor.mode(),
        anchor.rate_ppm(),
        anchor.offline_policy(),
    )
}

/// Monotonic timestamps are comparable only when both samples name one session.
pub fn validate_sample_after(sample: ClockSample, previous: ClockSample) -> Result<(), TimeError> {
    if sample.session_nonce() == previous.session_nonce()
        && sample.server_monotonic_ms() < previous.server_monotonic_ms()
    {
        Err(TimeError::NonMonotonicSample)
    } else {
        Ok(())
    }
}

/// Set a date and switch to TimeScale, preserving its current rate or using 1x.
pub fn set_world_time(
    anchor: ClockAnchor,
    at: MonotonicMs,
    world_utc_ms: WorldTimeMs,
) -> Result<ClockAnchor, TimeError> {
    let _current_time = evaluate_clock(anchor, at)?;
    let rate = match anchor.mode() {
        ClockMode::RealTime => TimeScalePpm::ONE_X,
        ClockMode::TimeScale => anchor.rate_ppm(),
    };
    reanchor(
        anchor,
        ClockMode::TimeScale,
        world_utc_ms,
        at,
        rate,
        anchor.offline_policy(),
    )
}

/// Set a rate and switch to TimeScale while keeping the evaluated world date.
pub fn set_time_scale(
    anchor: ClockAnchor,
    at: MonotonicMs,
    rate_ppm: TimeScalePpm,
) -> Result<ClockAnchor, TimeError> {
    let current_time = evaluate_clock(anchor, at)?;
    reanchor(
        anchor,
        ClockMode::TimeScale,
        current_time,
        at,
        rate_ppm,
        anchor.offline_policy(),
    )
}

/// Change mode without a discontinuity; RealTime requires UTC from its adapter.
pub fn set_clock_mode(
    anchor: ClockAnchor,
    at: MonotonicMs,
    mode: ClockMode,
    current_utc: Option<WorldTimeMs>,
) -> Result<ClockAnchor, TimeError> {
    let evaluated_time = evaluate_clock(anchor, at)?;
    let (world_time, rate) = match mode {
        ClockMode::RealTime => (
            current_utc.ok_or(TimeError::RealtimeUtcRequired)?,
            TimeScalePpm::ONE_X,
        ),
        ClockMode::TimeScale => {
            let rate = match anchor.mode() {
                ClockMode::RealTime => TimeScalePpm::ONE_X,
                ClockMode::TimeScale => anchor.rate_ppm(),
            };
            (evaluated_time, rate)
        }
    };
    reanchor(anchor, mode, world_time, at, rate, anchor.offline_policy())
}

/// Change only the restart policy, preserving evaluated date and clock mode.
pub fn set_offline_policy(
    anchor: ClockAnchor,
    at: MonotonicMs,
    offline_policy: OfflinePolicy,
) -> Result<ClockAnchor, TimeError> {
    let current_time = evaluate_clock(anchor, at)?;
    reanchor(
        anchor,
        anchor.mode(),
        current_time,
        at,
        anchor.rate_ppm(),
        offline_policy,
    )
}

/// Capture the clock at a save boundary without persisting process-local state.
pub fn save_clock_state(
    anchor: ClockAnchor,
    at: MonotonicMs,
    wall_utc_ms: WorldTimeMs,
) -> Result<SavedClockState, TimeError> {
    SavedClockState::new(
        anchor.clock_id(),
        anchor.revision(),
        anchor.mode(),
        evaluate_clock(anchor, at)?,
        wall_utc_ms,
        anchor.rate_ppm(),
        anchor.offline_policy(),
    )
}

/// Restore one saved clock. The saved revision is retained; the new session nonce
/// distinguishes monotonic timestamps across process restarts.
pub fn restore_clock_state(
    saved: SavedClockState,
    now_wall_utc_ms: WorldTimeMs,
    process_start: MonotonicMs,
) -> Result<RestoredClock, TimeError> {
    let wall_clock_went_backwards = now_wall_utc_ms < saved.saved_wall_utc_ms();
    let world_time = match (saved.mode(), saved.offline_policy()) {
        (ClockMode::RealTime, _) => now_wall_utc_ms,
        (ClockMode::TimeScale, OfflinePolicy::Pause) => saved.saved_world_utc_ms(),
        (ClockMode::TimeScale, OfflinePolicy::Advance) if wall_clock_went_backwards => {
            saved.saved_world_utc_ms()
        }
        (ClockMode::TimeScale, OfflinePolicy::Advance) => {
            let elapsed_ms = i128::from(now_wall_utc_ms.get())
                .checked_sub(i128::from(saved.saved_wall_utc_ms().get()))
                .ok_or(TimeError::ArithmeticOverflow)?;
            let advance_ms = scaled_elapsed(elapsed_ms, saved.rate_ppm())?;
            let result = i128::from(saved.saved_world_utc_ms().get())
                .checked_add(advance_ms)
                .ok_or(TimeError::ArithmeticOverflow)?;
            world_time_from_i128(result)?
        }
    };

    let anchor = ClockAnchor::new(
        saved.clock_id(),
        saved.revision(),
        saved.mode(),
        world_time,
        process_start,
        effective_rate(saved.mode(), saved.rate_ppm()),
        saved.offline_policy(),
    )?;
    Ok(RestoredClock {
        anchor,
        wall_clock_went_backwards,
    })
}

fn reanchor(
    previous: ClockAnchor,
    mode: ClockMode,
    world_utc_ms: WorldTimeMs,
    monotonic_ms: MonotonicMs,
    rate_ppm: TimeScalePpm,
    offline_policy: OfflinePolicy,
) -> Result<ClockAnchor, TimeError> {
    let revision = previous.revision().checked_next()?;
    ClockAnchor::new(
        previous.clock_id(),
        revision,
        mode,
        world_utc_ms,
        monotonic_ms,
        rate_ppm,
        offline_policy,
    )
}

fn monotonic_delta(start: MonotonicMs, end: MonotonicMs) -> Result<i128, TimeError> {
    if end < start {
        return Err(TimeError::NonMonotonicSample);
    }
    i128::from(end.get())
        .checked_sub(i128::from(start.get()))
        .ok_or(TimeError::ArithmeticOverflow)
}

fn scaled_elapsed(elapsed_ms: i128, rate_ppm: TimeScalePpm) -> Result<i128, TimeError> {
    if elapsed_ms < 0 {
        return Err(TimeError::NonMonotonicSample);
    }
    elapsed_ms
        .checked_mul(i128::from(rate_ppm.get()))
        .ok_or(TimeError::ArithmeticOverflow)
        .map(|scaled| scaled / SCALE_DENOMINATOR)
}

fn effective_rate(mode: ClockMode, requested: TimeScalePpm) -> TimeScalePpm {
    match mode {
        ClockMode::RealTime => TimeScalePpm::ONE_X,
        ClockMode::TimeScale => requested,
    }
}

fn world_time_from_i128(value: i128) -> Result<WorldTimeMs, TimeError> {
    if value < i128::from(MIN_WORLD_TIME_MS) || value >= i128::from(MAX_WORLD_TIME_MS_EXCLUSIVE) {
        return Err(TimeError::OutOfSupportedRange(value));
    }
    let value = i64::try_from(value).map_err(|_| TimeError::ArithmeticOverflow)?;
    WorldTimeMs::new(value)
}
