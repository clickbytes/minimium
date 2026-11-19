use super::EnvironmentConfig;
use super::{EnvironmentClockError, EnvironmentClockErrorKind};
use minimium_environment::time::SimulationLimit;
use minimium_environment::time::{
    restore_clock_state, set_clock_mode, set_offline_policy, set_time_scale, set_world_time,
    ClockAnchor, ClockMode, ClockRevision, ClockSample, MonotonicMs, OfflinePolicy,
    SavedClockState, SessionNonce, SimulationBoundary, TimeError, TimeScalePpm, WorldTimeMs,
};

#[derive(Clone, Debug)]
pub struct ClockPropertySnapshot {
    sample: ClockSample,
    pub(crate) tick: u64,
    pub(crate) achieved_ppm: u64,
    pub(crate) limit: &'static str,
    pub(crate) clock_id: u64,
    pub(crate) session_nonce: [u8; minimium_environment::time::SESSION_NONCE_BYTES],
    pub(crate) mode: &'static str,
    pub(crate) rate_ppm: u64,
    pub(crate) offline_policy: &'static str,
    pub(crate) utc_ms: i64,
    pub(crate) revision: u64,
    pub(crate) fault: &'static str,
    pub(crate) correction_ms: Option<i128>,
    pub(crate) dirty_revision: Option<u64>,
}
impl ClockPropertySnapshot {
    pub fn calendar_epoch(&self) -> u64 {
        self.sample.boundary().calendar_epoch().get()
    }
    pub fn status(&self) -> minimium_environment::console::status::TimeStatus {
        use minimium_environment::console::status::{ClockSource, TimeStatus};
        TimeStatus {
            sample: self.sample,
            source: ClockSource::OwnerSnapshot,
            quality: "Current",
            age_ms: Some(0),
        }
    }
    pub fn tick(&self) -> u64 {
        self.tick
    }
    pub fn achieved_ppm(&self) -> u64 {
        self.achieved_ppm
    }
    pub fn limit(&self) -> &'static str {
        self.limit
    }
    pub fn clock_id(&self) -> u64 {
        self.clock_id
    }
    pub fn session_nonce(&self) -> &[u8; minimium_environment::time::SESSION_NONCE_BYTES] {
        &self.session_nonce
    }
    pub fn mode(&self) -> &'static str {
        self.mode
    }
    pub fn rate_ppm(&self) -> u64 {
        self.rate_ppm
    }
    pub fn offline_policy(&self) -> &'static str {
        self.offline_policy
    }
    pub fn utc_ms(&self) -> i64 {
        self.utc_ms
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn fault(&self) -> &'static str {
        self.fault
    }
    pub fn correction_ms(&self) -> Option<i128> {
        self.correction_ms
    }
    pub fn dirty_revision(&self) -> Option<u64> {
        self.dirty_revision
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClockFault {
    TimeRangeExhausted,
    RevisionExhausted,
}

impl ClockFault {
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::TimeRangeExhausted => "time_range_exhausted",
            Self::RevisionExhausted => "clock_revision_exhausted",
        }
    }
}

pub(crate) struct EnvironmentClock {
    wall_utc: Result<WorldTimeMs, crate::RuntimeUtcError>,
    anchor: ClockAnchor,
    session_nonce: SessionNonce,
    boundary: SimulationBoundary,
    input_floor: u64,
    pub(crate) achieved_ppm: u64,
    pub(crate) limit: SimulationLimit,
    fault: Option<ClockFault>,
    dirty_revision: Option<ClockRevision>,
}

impl EnvironmentClock {
    pub(crate) fn set_host_utc(&mut self, utc: Result<WorldTimeMs, crate::RuntimeUtcError>) {
        self.wall_utc = utc;
    }
    pub(crate) fn new(
        config: EnvironmentConfig,
        saved_clock: Option<SavedClockState>,
        wall_utc: Result<WorldTimeMs, crate::RuntimeUtcError>,
        nonce: SessionNonce,
        logger: &minimium_diagnostics::DomainLogger,
    ) -> Result<Self, EnvironmentClockError> {
        let monotonic_ms = MonotonicMs::new(0);
        let mut range_fault = false;
        let mut boundary = saved_clock.map_or(SimulationBoundary::INITIAL, |s| s.boundary());
        let anchor = if let Some(saved) = saved_clock {
            let wall_utc_now = wall_utc.map_err(crate::RuntimeUtcError::environment_error)?;
            match restore_clock_state(saved, wall_utc_now, monotonic_ms) {
                Ok(restored) => {
                    if restored.wall_clock_went_backwards()
                        && saved.offline_policy() == OfflinePolicy::Advance
                    {
                        crate::diagnostics::emit(
                            logger,
                            minimium_diagnostics::Level::Warn,
                            "environment.offline_advance_skipped",
                            &[
                                (
                                    "saved_wall_utc_ms",
                                    saved.saved_wall_utc_ms().get().to_string(),
                                ),
                                ("current_wall_utc_ms", wall_utc_now.get().to_string()),
                            ],
                        );
                    }
                    let restored = restored.anchor();
                    if restored.world_utc_ms() != saved.saved_world_utc_ms() {
                        let revision = restored
                            .revision()
                            .checked_next()
                            .map_err(EnvironmentClockError::from_time)?;
                        boundary = SimulationBoundary::new(boundary.tick(), revision)
                            .map_err(EnvironmentClockError::from_time)?;
                        ClockAnchor::new(
                            restored.clock_id(),
                            revision,
                            restored.mode(),
                            restored.world_utc_ms(),
                            monotonic_ms,
                            restored.rate_ppm(),
                            restored.offline_policy(),
                        )
                        .map_err(EnvironmentClockError::from_time)?
                    } else {
                        restored
                    }
                }
                Err(TimeError::OutOfSupportedRange(_)) => {
                    range_fault = true;
                    ClockAnchor::new(
                        saved.clock_id(),
                        saved
                            .revision()
                            .checked_next()
                            .map_err(EnvironmentClockError::from_time)?,
                        ClockMode::TimeScale,
                        saved.saved_world_utc_ms(),
                        monotonic_ms,
                        TimeScalePpm::PAUSED,
                        saved.offline_policy(),
                    )
                    .map_err(EnvironmentClockError::from_time)?
                }
                Err(error) => {
                    return Err(EnvironmentClockError::caused_by(
                        EnvironmentClockErrorKind::InvalidSavedState,
                        "could not restore saved environment clock",
                        error,
                    ))
                }
            }
        } else {
            let world_time = match (config.mode, config.initial_utc) {
                (ClockMode::RealTime, _) => {
                    wall_utc.map_err(crate::RuntimeUtcError::environment_error)?
                }
                (ClockMode::TimeScale, Some(initial)) => initial,
                (ClockMode::TimeScale, None) => {
                    wall_utc.map_err(crate::RuntimeUtcError::environment_error)?
                }
            };
            ClockAnchor::initial(
                config.mode,
                world_time,
                monotonic_ms,
                config.scale_ppm,
                config.offline_policy,
            )
            .map_err(EnvironmentClockError::from_time)?
        };
        Ok(Self {
            wall_utc,
            anchor,
            session_nonce: nonce,
            boundary,
            input_floor: anchor.revision().get(),
            achieved_ppm: 0,
            limit: SimulationLimit::None,
            fault: range_fault.then_some(ClockFault::TimeRangeExhausted),
            dirty_revision: None,
        })
    }

    fn current_limit(&self) -> SimulationLimit {
        if self.fault == Some(ClockFault::TimeRangeExhausted) {
            SimulationLimit::UtcRange
        } else {
            self.limit
        }
    }
    pub(crate) fn accepts_input(&self, revision: u64) -> bool {
        (self.input_floor..=self.anchor.revision().get()).contains(&revision)
            && self.rate().get() != 0
    }
    pub(crate) fn boundary(&self) -> SimulationBoundary {
        self.boundary
    }
    pub(crate) fn rate(&self) -> TimeScalePpm {
        self.anchor.rate_ppm()
    }
    pub(crate) fn utc(&self) -> WorldTimeMs {
        self.anchor.world_utc_ms()
    }

    pub(crate) fn next_boundary(&mut self) -> Option<(SimulationBoundary, WorldTimeMs)> {
        if self.fault.is_some() || self.rate() == TimeScalePpm::PAUSED {
            return None;
        }
        let next =
            SimulationBoundary::new(self.boundary.tick() + 1, self.boundary.calendar_epoch());
        let utc = WorldTimeMs::new(self.utc().get() + SimulationBoundary::STEP_MS as i64);
        match (next, utc) {
            (Ok(next), Ok(utc)) => Some((next, utc)),
            _ => {
                self.pause_on_range_exhaustion(self.anchor.monotonic_ms());
                self.limit = SimulationLimit::UtcRange;
                None
            }
        }
    }

    pub(crate) fn commit(&mut self, boundary: SimulationBoundary, utc: WorldTimeMs) {
        self.boundary = boundary;
        self.anchor = ClockAnchor::new(
            self.anchor.clock_id(),
            self.anchor.revision(),
            self.anchor.mode(),
            utc,
            self.anchor.monotonic_ms(),
            self.rate(),
            self.anchor.offline_policy(),
        )
        .expect("validated clock configuration");
    }

    pub(crate) fn sample(&self, now: MonotonicMs) -> Result<ClockSample, EnvironmentClockError> {
        if self.fault == Some(ClockFault::RevisionExhausted) {
            return Err(EnvironmentClockError::fault(
                EnvironmentClockErrorKind::RevisionExhausted,
                ClockFault::RevisionExhausted.code(),
            ));
        }
        ClockSample::new(
            self.anchor.clock_id(),
            self.anchor.revision(),
            self.session_nonce,
            now,
            self.utc(),
            self.anchor.mode(),
            self.rate(),
            self.anchor.offline_policy(),
        )
        .map(|sample| {
            sample
                .with_boundary(self.boundary)
                .with_progress(self.achieved_ppm, self.current_limit())
        })
        .map_err(EnvironmentClockError::from_time)
    }

    pub(crate) fn property_snapshot(
        &self,
        now: MonotonicMs,
    ) -> Result<ClockPropertySnapshot, EnvironmentClockError> {
        let sample = self.sample(now)?;
        Ok(ClockPropertySnapshot {
            sample,
            clock_id: sample.clock_id().get(),
            session_nonce: *self.session_nonce.as_bytes(),
            mode: mode_name(sample.mode()),
            rate_ppm: sample.rate_ppm().get(),
            offline_policy: policy_name(sample.offline_policy()),
            utc_ms: sample.world_utc_ms().get(),
            revision: sample.revision().get(),
            fault: self.fault.map_or("none", ClockFault::code),
            correction_ms: self
                .wall_utc
                .ok()
                .filter(|_| self.anchor.mode() == ClockMode::RealTime)
                .map(|wall| i128::from(wall.get()) - i128::from(self.utc().get())),
            tick: self.boundary.tick(),
            achieved_ppm: self.achieved_ppm,
            limit: self.current_limit().label(),
            dirty_revision: self.dirty_revision.map(|revision| revision.get()),
        })
    }

    pub(crate) fn capture_saved_state(
        &mut self,
        _now: MonotonicMs,
    ) -> Result<SavedClockState, EnvironmentClockError> {
        if self.fault == Some(ClockFault::RevisionExhausted) {
            return Err(EnvironmentClockError::fault(
                EnvironmentClockErrorKind::RevisionExhausted,
                ClockFault::RevisionExhausted.code(),
            ));
        }
        let wall_utc = self
            .wall_utc
            .map_err(crate::RuntimeUtcError::environment_error)?;
        SavedClockState::new(
            self.anchor.clock_id(),
            self.anchor.revision(),
            self.anchor.mode(),
            self.utc(),
            wall_utc,
            self.rate(),
            self.anchor.offline_policy(),
        )
        .map(|saved| saved.with_boundary(self.boundary))
        .map_err(EnvironmentClockError::from_time)
    }

    pub(crate) fn set_date(
        &mut self,
        now: MonotonicMs,
        value: WorldTimeMs,
    ) -> Result<ClockRevision, EnvironmentClockError> {
        self.ensure_mutable(now, true)?;
        let anchor = map_clock_error(set_world_time(self.anchor, value), &mut self.fault)?;
        self.replace_anchor(anchor, true);
        Ok(anchor.revision())
    }

    pub(crate) fn set_rate(
        &mut self,
        now: MonotonicMs,
        value: TimeScalePpm,
    ) -> Result<ClockRevision, EnvironmentClockError> {
        self.ensure_mutable(now, false)?;
        let anchor = map_clock_error(set_time_scale(self.anchor, value), &mut self.fault)?;
        self.replace_anchor(anchor, false);
        Ok(anchor.revision())
    }

    pub(crate) fn set_mode(
        &mut self,
        now: MonotonicMs,
        value: ClockMode,
    ) -> Result<ClockRevision, EnvironmentClockError> {
        let recover = value == ClockMode::RealTime;
        self.ensure_mutable(now, recover)?;
        let utc_now = if recover {
            Some(
                self.wall_utc
                    .map_err(crate::RuntimeUtcError::environment_error)?,
            )
        } else {
            None
        };
        let anchor = map_clock_error(set_clock_mode(self.anchor, value, utc_now), &mut self.fault)?;
        self.replace_anchor(anchor, recover);
        Ok(anchor.revision())
    }

    pub(crate) fn set_offline_policy(
        &mut self,
        now: MonotonicMs,
        value: OfflinePolicy,
    ) -> Result<ClockRevision, EnvironmentClockError> {
        self.ensure_mutable(now, false)?;
        let anchor = map_clock_error(set_offline_policy(self.anchor, value), &mut self.fault)?;
        self.replace_anchor(anchor, false);
        Ok(anchor.revision())
    }

    pub(crate) const fn dirty_revision(&self) -> Option<ClockRevision> {
        self.dirty_revision
    }

    fn ensure_mutable(
        &mut self,
        _now: MonotonicMs,
        recovers_range_fault: bool,
    ) -> Result<(), EnvironmentClockError> {
        match self.fault {
            Some(ClockFault::RevisionExhausted) => {
                Err(EnvironmentClockError::fault(EnvironmentClockErrorKind::RevisionExhausted, ClockFault::RevisionExhausted.code()))
            }
            Some(ClockFault::TimeRangeExhausted) if !recovers_range_fault => Err(EnvironmentClockError::fault(EnvironmentClockErrorKind::TimeRange, "clock is paused after time_range_exhausted; set a supported date or switch to real_time")),
            _ => Ok(()),
        }
    }

    fn replace_anchor(&mut self, anchor: ClockAnchor, clear_fault: bool) {
        if self.rate() == TimeScalePpm::PAUSED && anchor.rate_ppm() != TimeScalePpm::PAUSED {
            self.input_floor = anchor.revision().get();
        }
        self.anchor = anchor;
        if clear_fault {
            self.boundary = SimulationBoundary::new(self.boundary.tick(), anchor.revision())
                .expect("validated tick");
        }
        if clear_fault {
            self.fault = None;
        }
        self.dirty_revision = Some(anchor.revision());
    }

    fn pause_on_range_exhaustion(&mut self, now: MonotonicMs) -> ClockFault {
        if let Some(fault) = self.fault {
            return fault;
        }
        let revision = match self.anchor.revision().checked_next() {
            Ok(value) => value,
            Err(_) => {
                self.fault = Some(ClockFault::RevisionExhausted);
                return ClockFault::RevisionExhausted;
            }
        };
        let anchor = ClockAnchor::new(
            self.anchor.clock_id(),
            revision,
            ClockMode::TimeScale,
            self.utc(),
            now,
            TimeScalePpm::PAUSED,
            self.anchor.offline_policy(),
        );
        match anchor {
            Ok(anchor) => {
                self.anchor = anchor;
                self.fault = Some(ClockFault::TimeRangeExhausted);
                self.dirty_revision = Some(anchor.revision());
                ClockFault::TimeRangeExhausted
            }
            Err(_) => {
                self.fault = Some(ClockFault::RevisionExhausted);
                ClockFault::RevisionExhausted
            }
        }
    }
}

fn map_clock_error(
    result: Result<ClockAnchor, TimeError>,
    fault: &mut Option<ClockFault>,
) -> Result<ClockAnchor, EnvironmentClockError> {
    result.map_err(|error| {
        if error == TimeError::RevisionExhausted {
            *fault = Some(ClockFault::RevisionExhausted);
        }
        EnvironmentClockError::from_time(error)
    })
}

fn mode_name(mode: ClockMode) -> &'static str {
    match mode {
        ClockMode::RealTime => "real_time",
        ClockMode::TimeScale => "time_scale",
    }
}

fn policy_name(policy: OfflinePolicy) -> &'static str {
    match policy {
        OfflinePolicy::Pause => "pause",
        OfflinePolicy::Advance => "advance",
    }
}
