use super::EnvironmentConfig;
use super::{EnvironmentClockError, EnvironmentClockErrorKind};
use minimium_environment::time::{
    evaluate_clock, restore_clock_state, sample_clock, save_clock_state, set_clock_mode,
    set_offline_policy, set_time_scale, set_world_time, ClockAnchor, ClockMode, ClockRevision,
    ClockSample, MonotonicMs, OfflinePolicy, SavedClockState, SessionNonce, TimeError,
    TimeScalePpm, WorldTimeMs,
};

const SYSTEM_CLOCK_CHECK_MS: i128 = 250;

#[derive(Clone, Debug)]
pub struct ClockPropertySnapshot {
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
    last_valid_world_time: WorldTimeMs,
    last_correction_ms: Option<i128>,
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
        let anchor = if let Some(saved) = saved_clock {
            let wall_utc_now = wall_utc.map_err(crate::RuntimeUtcError::environment_error)?;
            let restored =
                restore_clock_state(saved, wall_utc_now, monotonic_ms).map_err(|error| {
                    EnvironmentClockError::caused_by(
                        EnvironmentClockErrorKind::InvalidSavedState,
                        "could not restore saved environment clock",
                        error,
                    )
                })?;
            if saved.mode() == ClockMode::TimeScale
                && saved.offline_policy() == OfflinePolicy::Advance
                && restored.wall_clock_went_backwards()
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
            restored.anchor()
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
            last_valid_world_time: anchor.world_utc_ms(),
            last_correction_ms: None,
            fault: None,
            dirty_revision: None,
        })
    }

    pub(crate) fn observe(
        &mut self,
        now: MonotonicMs,
    ) -> Result<Option<ClockFault>, EnvironmentClockError> {
        if self.fault == Some(ClockFault::RevisionExhausted) {
            return Ok(None);
        }
        match evaluate_clock(self.anchor, now) {
            Ok(world_time) => {
                self.last_valid_world_time = world_time;
                Ok(None)
            }
            Err(TimeError::OutOfSupportedRange(_)) => Ok(Some(self.pause_on_range_exhaustion(now))),
            Err(error) => Err(EnvironmentClockError::from_time(error)),
        }
    }

    pub(crate) fn check_system_clock(
        &mut self,
        now: MonotonicMs,
    ) -> Result<Option<i128>, EnvironmentClockError> {
        if self.anchor.mode() != ClockMode::RealTime
            || self.fault == Some(ClockFault::RevisionExhausted)
        {
            return Ok(None);
        }
        let utc_now = match self.wall_utc {
            Ok(utc) => utc,
            Err(error) => {
                let fault = self.pause_on_range_exhaustion(now);
                return Err(EnvironmentClockError::caused_by(
                    EnvironmentClockErrorKind::SystemClock,
                    fault.code(),
                    error.environment_error(),
                ));
            }
        };
        let expected = match evaluate_clock(self.anchor, now) {
            Ok(value) => value,
            Err(TimeError::OutOfSupportedRange(_)) => {
                let fault = self.pause_on_range_exhaustion(now);
                return Err(EnvironmentClockError::fault(
                    EnvironmentClockErrorKind::TimeRange,
                    fault.code(),
                ));
            }
            Err(error) => return Err(EnvironmentClockError::from_time(error)),
        };
        let correction_ms = i128::from(utc_now.get()) - i128::from(expected.get());
        if correction_ms.abs() < SYSTEM_CLOCK_CHECK_MS {
            self.last_valid_world_time = expected;
            return Ok(None);
        }
        match set_clock_mode(self.anchor, now, ClockMode::RealTime, Some(utc_now)) {
            Ok(anchor) => {
                self.anchor = anchor;
                self.last_valid_world_time = utc_now;
                self.last_correction_ms = Some(correction_ms);
                self.dirty_revision = Some(anchor.revision());
                Ok(Some(correction_ms))
            }
            Err(TimeError::RevisionExhausted) => {
                self.fault = Some(ClockFault::RevisionExhausted);
                Err(EnvironmentClockError::fault(
                    EnvironmentClockErrorKind::RevisionExhausted,
                    ClockFault::RevisionExhausted.code(),
                ))
            }
            Err(error) => Err(EnvironmentClockError::from_time(error)),
        }
    }

    pub(crate) fn sample(&self, now: MonotonicMs) -> Result<ClockSample, EnvironmentClockError> {
        if self.fault == Some(ClockFault::RevisionExhausted) {
            return Err(EnvironmentClockError::fault(
                EnvironmentClockErrorKind::RevisionExhausted,
                ClockFault::RevisionExhausted.code(),
            ));
        }
        sample_clock(self.anchor, self.session_nonce, now).map_err(EnvironmentClockError::from_time)
    }

    pub(crate) fn property_snapshot(
        &self,
        now: MonotonicMs,
    ) -> Result<ClockPropertySnapshot, EnvironmentClockError> {
        let sample = self.sample(now)?;
        Ok(ClockPropertySnapshot {
            clock_id: sample.clock_id().get(),
            session_nonce: *self.session_nonce.as_bytes(),
            mode: mode_name(sample.mode()),
            rate_ppm: sample.rate_ppm().get(),
            offline_policy: policy_name(sample.offline_policy()),
            utc_ms: sample.world_utc_ms().get(),
            revision: sample.revision().get(),
            fault: self.fault.map(ClockFault::code).unwrap_or_else(|| {
                if matches!(
                    evaluate_clock(self.anchor, now),
                    Err(TimeError::OutOfSupportedRange(_))
                ) {
                    "time_range_exhausted_pending"
                } else {
                    "none"
                }
            }),
            correction_ms: self.last_correction_ms,
            dirty_revision: self.dirty_revision.map(|revision| revision.get()),
        })
    }

    pub(crate) fn capture_saved_state(
        &mut self,
        now: MonotonicMs,
    ) -> Result<SavedClockState, EnvironmentClockError> {
        self.observe(now)?;
        if self.fault == Some(ClockFault::RevisionExhausted) {
            return Err(EnvironmentClockError::fault(
                EnvironmentClockErrorKind::RevisionExhausted,
                ClockFault::RevisionExhausted.code(),
            ));
        }
        let wall_utc = self
            .wall_utc
            .map_err(crate::RuntimeUtcError::environment_error)?;
        save_clock_state(self.anchor, now, wall_utc).map_err(EnvironmentClockError::from_time)
    }

    pub(crate) fn set_date(
        &mut self,
        now: MonotonicMs,
        value: WorldTimeMs,
    ) -> Result<ClockRevision, EnvironmentClockError> {
        self.ensure_mutable(now, true)?;
        let anchor = map_clock_error(set_world_time(self.anchor, now, value), &mut self.fault)?;
        self.replace_anchor(anchor, true);
        Ok(anchor.revision())
    }

    pub(crate) fn set_rate(
        &mut self,
        now: MonotonicMs,
        value: TimeScalePpm,
    ) -> Result<ClockRevision, EnvironmentClockError> {
        self.ensure_mutable(now, false)?;
        let anchor = map_clock_error(set_time_scale(self.anchor, now, value), &mut self.fault)?;
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
        let anchor = map_clock_error(
            set_clock_mode(self.anchor, now, value, utc_now),
            &mut self.fault,
        )?;
        self.replace_anchor(anchor, recover);
        Ok(anchor.revision())
    }

    pub(crate) fn set_offline_policy(
        &mut self,
        now: MonotonicMs,
        value: OfflinePolicy,
    ) -> Result<ClockRevision, EnvironmentClockError> {
        self.ensure_mutable(now, false)?;
        let anchor = map_clock_error(set_offline_policy(self.anchor, now, value), &mut self.fault)?;
        self.replace_anchor(anchor, false);
        Ok(anchor.revision())
    }

    pub(crate) const fn dirty_revision(&self) -> Option<ClockRevision> {
        self.dirty_revision
    }

    fn ensure_mutable(
        &mut self,
        now: MonotonicMs,
        recovers_range_fault: bool,
    ) -> Result<(), EnvironmentClockError> {
        self.observe(now)?;
        match self.fault {
            Some(ClockFault::RevisionExhausted) => {
                Err(EnvironmentClockError::fault(EnvironmentClockErrorKind::RevisionExhausted, ClockFault::RevisionExhausted.code()))
            }
            Some(ClockFault::TimeRangeExhausted) if !recovers_range_fault => Err(EnvironmentClockError::fault(EnvironmentClockErrorKind::TimeRange, "clock is paused after time_range_exhausted; set a supported date or switch to real_time")),
            _ => Ok(()),
        }
    }

    fn replace_anchor(&mut self, anchor: ClockAnchor, clear_fault: bool) {
        self.anchor = anchor;
        self.last_valid_world_time = anchor.world_utc_ms();
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
            self.last_valid_world_time,
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
