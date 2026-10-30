use std::fmt;

pub const MIN_WORLD_TIME_MS: i64 = 946_684_800_000;
pub const MAX_WORLD_TIME_MS_EXCLUSIVE: i64 = 4_133_980_800_000;
pub const ONE_X_PPM: u64 = 1_000_000;
pub const MAX_TIME_SCALE_PPM: u64 = 1_000_000_000_000;
pub const SESSION_NONCE_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimeError {
    OutOfSupportedRange(i128),
    InvalidScale(u64),
    InvalidMode(u8),
    InvalidPolicy(u8),
    UnsupportedClockId(u64),
    ZeroRevision,
    NonMonotonicSample,
    ArithmeticOverflow,
    RevisionExhausted,
    RealtimeUtcRequired,
    InvalidRealtimeScale,
}

impl fmt::Display for TimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfSupportedRange(value) => {
                write!(
                    formatter,
                    "world time {value} ms is outside the supported range"
                )
            }
            Self::InvalidScale(value) => write!(formatter, "unsupported time scale {value} ppm"),
            Self::InvalidMode(value) => write!(formatter, "unsupported clock mode {value}"),
            Self::InvalidPolicy(value) => write!(formatter, "unsupported offline policy {value}"),
            Self::UnsupportedClockId(value) => write!(formatter, "unsupported clock id {value}"),
            Self::ZeroRevision => formatter.write_str("clock revision must be nonzero"),
            Self::NonMonotonicSample => {
                formatter.write_str("monotonic timestamp precedes its clock anchor or sample")
            }
            Self::ArithmeticOverflow => formatter.write_str("clock arithmetic overflowed"),
            Self::RevisionExhausted => formatter.write_str("clock revision is exhausted"),
            Self::RealtimeUtcRequired => {
                formatter.write_str("switching to real time requires a current UTC value")
            }
            Self::InvalidRealtimeScale => {
                formatter.write_str("real-time mode must use an effective scale of 1x")
            }
        }
    }
}

impl std::error::Error for TimeError {}

/// MVP clock shared by the planet and interior instances.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClockId(u64);

impl ClockId {
    pub const MVP: Self = Self(1);

    pub const fn new(value: u64) -> Result<Self, TimeError> {
        if value == 1 {
            Ok(Self::MVP)
        } else {
            Err(TimeError::UnsupportedClockId(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for ClockId {
    type Error = TimeError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Monotonically increasing revision of one clock's configuration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ClockRevision(u64);

impl ClockRevision {
    pub const INITIAL: Self = Self(1);

    pub const fn new(value: u64) -> Result<Self, TimeError> {
        if value == 0 {
            Err(TimeError::ZeroRevision)
        } else {
            Ok(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    pub const fn checked_next(self) -> Result<Self, TimeError> {
        match self.0.checked_add(1) {
            Some(value) => Ok(Self(value)),
            None => Err(TimeError::RevisionExhausted),
        }
    }
}

impl TryFrom<u64> for ClockRevision {
    type Error = TimeError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// POSIX UTC milliseconds, bounded to the supported astronomical date range.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorldTimeMs(i64);

impl WorldTimeMs {
    pub const MIN: Self = Self(MIN_WORLD_TIME_MS);
    pub const MAX_EXCLUSIVE: i64 = MAX_WORLD_TIME_MS_EXCLUSIVE;

    pub const fn new(value: i64) -> Result<Self, TimeError> {
        if value < MIN_WORLD_TIME_MS || value >= MAX_WORLD_TIME_MS_EXCLUSIVE {
            Err(TimeError::OutOfSupportedRange(value as i128))
        } else {
            Ok(Self(value))
        }
    }

    pub const fn get(self) -> i64 {
        self.0
    }
}

impl TryFrom<i64> for WorldTimeMs {
    type Error = TimeError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Milliseconds from the beginning of one process session; never a wall clock.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MonotonicMs(u64);

impl MonotonicMs {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SessionNonce([u8; SESSION_NONCE_BYTES]);

impl SessionNonce {
    pub const fn new(bytes: [u8; SESSION_NONCE_BYTES]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; SESSION_NONCE_BYTES] {
        &self.0
    }
}

/// A nonnegative multiplier in parts per million; zero pauses the world clock.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TimeScalePpm(u64);

impl TimeScalePpm {
    pub const PAUSED: Self = Self(0);
    pub const ONE_X: Self = Self(ONE_X_PPM);
    pub const MAX: Self = Self(MAX_TIME_SCALE_PPM);

    pub const fn new(value: u64) -> Result<Self, TimeError> {
        if value > MAX_TIME_SCALE_PPM {
            Err(TimeError::InvalidScale(value))
        } else {
            Ok(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for TimeScalePpm {
    type Error = TimeError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum ClockMode {
    RealTime = 0,
    TimeScale = 1,
}

impl TryFrom<u8> for ClockMode {
    type Error = TimeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::RealTime),
            1 => Ok(Self::TimeScale),
            other => Err(TimeError::InvalidMode(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum OfflinePolicy {
    #[default]
    Pause = 0,
    Advance = 1,
}

impl TryFrom<u8> for OfflinePolicy {
    type Error = TimeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Pause),
            1 => Ok(Self::Advance),
            other => Err(TimeError::InvalidPolicy(other)),
        }
    }
}

/// Immutable anchor used to evaluate world time without accumulated frame deltas.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockAnchor {
    clock_id: ClockId,
    revision: ClockRevision,
    mode: ClockMode,
    world_utc_ms: WorldTimeMs,
    monotonic_ms: MonotonicMs,
    rate_ppm: TimeScalePpm,
    offline_policy: OfflinePolicy,
}

impl ClockAnchor {
    pub const fn initial(
        mode: ClockMode,
        world_utc_ms: WorldTimeMs,
        monotonic_ms: MonotonicMs,
        rate_ppm: TimeScalePpm,
        offline_policy: OfflinePolicy,
    ) -> Result<Self, TimeError> {
        Self::new(
            ClockId::MVP,
            ClockRevision::INITIAL,
            mode,
            world_utc_ms,
            monotonic_ms,
            rate_ppm,
            offline_policy,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        clock_id: ClockId,
        revision: ClockRevision,
        mode: ClockMode,
        world_utc_ms: WorldTimeMs,
        monotonic_ms: MonotonicMs,
        rate_ppm: TimeScalePpm,
        offline_policy: OfflinePolicy,
    ) -> Result<Self, TimeError> {
        if matches!(mode, ClockMode::RealTime) && rate_ppm.get() != ONE_X_PPM {
            return Err(TimeError::InvalidRealtimeScale);
        }
        Ok(Self {
            clock_id,
            revision,
            mode,
            world_utc_ms,
            monotonic_ms,
            rate_ppm,
            offline_policy,
        })
    }

    pub const fn clock_id(self) -> ClockId {
        self.clock_id
    }

    pub const fn revision(self) -> ClockRevision {
        self.revision
    }

    pub const fn mode(self) -> ClockMode {
        self.mode
    }

    pub const fn world_utc_ms(self) -> WorldTimeMs {
        self.world_utc_ms
    }

    pub const fn monotonic_ms(self) -> MonotonicMs {
        self.monotonic_ms
    }

    pub const fn rate_ppm(self) -> TimeScalePpm {
        self.rate_ppm
    }

    pub const fn offline_policy(self) -> OfflinePolicy {
        self.offline_policy
    }
}

/// Server sample is valid only in the process session named by its nonce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockSample {
    clock_id: ClockId,
    revision: ClockRevision,
    session_nonce: SessionNonce,
    server_monotonic_ms: MonotonicMs,
    world_utc_ms: WorldTimeMs,
    mode: ClockMode,
    rate_ppm: TimeScalePpm,
    offline_policy: OfflinePolicy,
}

impl ClockSample {
    /// `world_utc_ms` must be evaluated for exactly `server_monotonic_ms`.
    /// Authoritative servers should prefer [`super::evaluation::sample_clock`].
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        clock_id: ClockId,
        revision: ClockRevision,
        session_nonce: SessionNonce,
        server_monotonic_ms: MonotonicMs,
        world_utc_ms: WorldTimeMs,
        mode: ClockMode,
        rate_ppm: TimeScalePpm,
        offline_policy: OfflinePolicy,
    ) -> Result<Self, TimeError> {
        if matches!(mode, ClockMode::RealTime) && rate_ppm.get() != ONE_X_PPM {
            return Err(TimeError::InvalidRealtimeScale);
        }
        Ok(Self {
            clock_id,
            revision,
            session_nonce,
            server_monotonic_ms,
            world_utc_ms,
            mode,
            rate_ppm,
            offline_policy,
        })
    }

    pub const fn clock_id(self) -> ClockId {
        self.clock_id
    }

    pub const fn revision(self) -> ClockRevision {
        self.revision
    }

    pub const fn session_nonce(self) -> SessionNonce {
        self.session_nonce
    }

    pub const fn server_monotonic_ms(self) -> MonotonicMs {
        self.server_monotonic_ms
    }

    pub const fn world_utc_ms(self) -> WorldTimeMs {
        self.world_utc_ms
    }

    pub const fn mode(self) -> ClockMode {
        self.mode
    }

    pub const fn rate_ppm(self) -> TimeScalePpm {
        self.rate_ppm
    }

    pub const fn offline_policy(self) -> OfflinePolicy {
        self.offline_policy
    }
}

/// Persisted clock state intentionally excludes process nonce and monotonic time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SavedClockState {
    clock_id: ClockId,
    revision: ClockRevision,
    mode: ClockMode,
    saved_world_utc_ms: WorldTimeMs,
    saved_wall_utc_ms: WorldTimeMs,
    rate_ppm: TimeScalePpm,
    offline_policy: OfflinePolicy,
}

impl SavedClockState {
    pub const fn new(
        clock_id: ClockId,
        revision: ClockRevision,
        mode: ClockMode,
        saved_world_utc_ms: WorldTimeMs,
        saved_wall_utc_ms: WorldTimeMs,
        rate_ppm: TimeScalePpm,
        offline_policy: OfflinePolicy,
    ) -> Result<Self, TimeError> {
        if matches!(mode, ClockMode::RealTime) && rate_ppm.get() != ONE_X_PPM {
            return Err(TimeError::InvalidRealtimeScale);
        }
        Ok(Self {
            clock_id,
            revision,
            mode,
            saved_world_utc_ms,
            saved_wall_utc_ms,
            rate_ppm,
            offline_policy,
        })
    }

    pub const fn clock_id(self) -> ClockId {
        self.clock_id
    }

    pub const fn revision(self) -> ClockRevision {
        self.revision
    }

    pub const fn mode(self) -> ClockMode {
        self.mode
    }

    pub const fn saved_world_utc_ms(self) -> WorldTimeMs {
        self.saved_world_utc_ms
    }

    pub const fn saved_wall_utc_ms(self) -> WorldTimeMs {
        self.saved_wall_utc_ms
    }

    pub const fn rate_ppm(self) -> TimeScalePpm {
        self.rate_ppm
    }

    pub const fn offline_policy(self) -> OfflinePolicy {
        self.offline_policy
    }
}
