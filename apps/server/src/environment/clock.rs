use super::EnvironmentConfig;
use minimium_core::time::{
    evaluate_clock, sample_clock, set_clock_mode, set_offline_policy, set_time_scale,
    set_world_time, ClockAnchor, ClockMode, ClockRevision, ClockSample, MonotonicMs, OfflinePolicy,
    SessionNonce, TimeError, TimeScalePpm, WorldTimeMs,
};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const SYSTEM_CLOCK_CHECK_MS: i128 = 250;

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
    anchor: ClockAnchor,
    session_nonce: SessionNonce,
    last_valid_world_time: WorldTimeMs,
    last_correction_ms: Option<i128>,
    fault: Option<ClockFault>,
    dirty_revision: Option<ClockRevision>,
}

impl EnvironmentClock {
    pub(crate) fn new(config: EnvironmentConfig, server_started: Instant) -> Result<Self, String> {
        let nonce = minimium_platform::endpoint::new_random_bytes::<16>()
            .map(SessionNonce::new)
            .map_err(|error| format!("could not create server session nonce: {error}"))?;
        let world_time = match (config.mode, config.initial_utc) {
            (ClockMode::RealTime, _) => system_world_time_now()?,
            (ClockMode::TimeScale, Some(initial)) => initial,
            (ClockMode::TimeScale, None) => system_world_time_now()?,
        };
        let monotonic_ms = u64::try_from(server_started.elapsed().as_millis())
            .map(MonotonicMs::new)
            .map_err(|_| "server monotonic time overflow".to_owned())?;
        let anchor = ClockAnchor::initial(
            config.mode,
            world_time,
            monotonic_ms,
            config.scale_ppm,
            config.offline_policy,
        )
        .map_err(|error| error.to_string())?;
        Ok(Self {
            anchor,
            session_nonce: nonce,
            last_valid_world_time: world_time,
            last_correction_ms: None,
            fault: None,
            dirty_revision: None,
        })
    }

    pub(crate) fn observe(&mut self, now: MonotonicMs) -> Option<ClockFault> {
        if self.fault == Some(ClockFault::RevisionExhausted) {
            return None;
        }
        match evaluate_clock(self.anchor, now) {
            Ok(world_time) => {
                self.last_valid_world_time = world_time;
                None
            }
            Err(TimeError::OutOfSupportedRange(_)) => Some(self.pause_on_range_exhaustion(now)),
            Err(_) => None,
        }
    }

    pub(crate) fn check_system_clock(&mut self, now: MonotonicMs) -> Result<Option<i128>, String> {
        if self.anchor.mode() != ClockMode::RealTime
            || self.fault == Some(ClockFault::RevisionExhausted)
        {
            return Ok(None);
        }
        let utc_now = match system_world_time_now() {
            Ok(value) => value,
            Err(_) => {
                let fault = self.pause_on_range_exhaustion(now);
                return Err(fault.code().to_owned());
            }
        };
        let expected = match evaluate_clock(self.anchor, now) {
            Ok(value) => value,
            Err(TimeError::OutOfSupportedRange(_)) => {
                let fault = self.pause_on_range_exhaustion(now);
                return Err(fault.code().to_owned());
            }
            Err(error) => return Err(error.to_string()),
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
                Err(ClockFault::RevisionExhausted.code().to_owned())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    pub(crate) fn status(&self, now: MonotonicMs) -> String {
        let world_utc_ms = self
            .sample(now)
            .map(|sample| sample.world_utc_ms().get())
            .unwrap_or(self.last_valid_world_time.get());
        let fault = self.fault.map(ClockFault::code).unwrap_or_else(|| {
            if matches!(
                evaluate_clock(self.anchor, now),
                Err(TimeError::OutOfSupportedRange(_))
            ) {
                "time_range_exhausted_pending"
            } else {
                "none"
            }
        });
        let correction = self
            .last_correction_ms
            .map(format_correction)
            .unwrap_or_else(|| "none".to_owned());
        let dirty = self
            .dirty_revision
            .map(|revision| revision.get().to_string())
            .unwrap_or_else(|| "none".to_owned());
        format!(
            "clock_id={} revision={} session_nonce={} mode={} world_utc_ms={} rate_ppm={} rate={} offline_policy={} fault={} last_utc_correction_ms={} sync_dirty_revision={}",
            self.anchor.clock_id().get(),
            self.anchor.revision().get(),
            nonce_hex(self.session_nonce),
            mode_name(self.anchor.mode()),
            world_utc_ms,
            self.anchor.rate_ppm().get(),
            format_scale(self.anchor.rate_ppm()),
            policy_name(self.anchor.offline_policy()),
            fault,
            correction,
            dirty,
        )
    }

    pub(crate) fn sample(&self, now: MonotonicMs) -> Result<ClockSample, String> {
        if self.fault == Some(ClockFault::RevisionExhausted) {
            return Err(ClockFault::RevisionExhausted.code().to_owned());
        }
        sample_clock(self.anchor, self.session_nonce, now).map_err(|error| error.to_string())
    }

    pub(crate) fn set_date(
        &mut self,
        now: MonotonicMs,
        value: WorldTimeMs,
    ) -> Result<ClockRevision, String> {
        self.ensure_mutable(now, true)?;
        let anchor = map_clock_error(set_world_time(self.anchor, now, value), &mut self.fault)?;
        self.replace_anchor(anchor, true);
        Ok(anchor.revision())
    }

    pub(crate) fn set_rate(
        &mut self,
        now: MonotonicMs,
        value: TimeScalePpm,
    ) -> Result<ClockRevision, String> {
        self.ensure_mutable(now, false)?;
        let anchor = map_clock_error(set_time_scale(self.anchor, now, value), &mut self.fault)?;
        self.replace_anchor(anchor, false);
        Ok(anchor.revision())
    }

    pub(crate) fn set_mode(
        &mut self,
        now: MonotonicMs,
        value: ClockMode,
    ) -> Result<ClockRevision, String> {
        let recover = value == ClockMode::RealTime;
        self.ensure_mutable(now, recover)?;
        let utc_now = if recover {
            Some(system_world_time_now()?)
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
    ) -> Result<ClockRevision, String> {
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
    ) -> Result<(), String> {
        self.observe(now);
        match self.fault {
            Some(ClockFault::RevisionExhausted) => {
                Err(ClockFault::RevisionExhausted.code().to_owned())
            }
            Some(ClockFault::TimeRangeExhausted) if !recovers_range_fault => Err(
                "clock is paused after time_range_exhausted; set a supported date or switch to real_time".to_owned(),
            ),
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
) -> Result<ClockAnchor, String> {
    result.map_err(|error| {
        if error == TimeError::RevisionExhausted {
            *fault = Some(ClockFault::RevisionExhausted);
        }
        error.to_string()
    })
}

fn system_world_time_now() -> Result<WorldTimeMs, String> {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before the POSIX epoch".to_owned())?;
    let millis = i128::try_from(elapsed.as_millis())
        .map_err(|_| "system UTC milliseconds overflow i128".to_owned())?;
    let millis =
        i64::try_from(millis).map_err(|_| "system UTC milliseconds overflow i64".to_owned())?;
    WorldTimeMs::new(millis)
        .map_err(|error| format!("system UTC is outside supported range: {error}"))
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

fn format_scale(rate: TimeScalePpm) -> String {
    let whole = rate.get() / 1_000_000;
    let fraction = rate.get() % 1_000_000;
    format!("{whole}.{fraction:06}x")
}

fn format_correction(offset_ms: i128) -> String {
    format!("{offset_ms:+}")
}

fn nonce_hex(nonce: SessionNonce) -> String {
    let mut text = String::with_capacity(32);
    for byte in nonce.as_bytes() {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
    }
    text
}
