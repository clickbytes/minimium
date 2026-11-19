use super::types::*;

/// The simulation duration is exactly reconstructible from the committed tick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SimulationBoundary {
    tick: u64,
    calendar_epoch: ClockRevision,
}

impl SimulationBoundary {
    pub const STEP_MS: u64 = 50;
    pub const INITIAL: Self = Self {
        tick: 0,
        calendar_epoch: ClockRevision::INITIAL,
    };

    pub fn new(tick: u64, calendar_epoch: ClockRevision) -> Result<Self, TimeError> {
        tick.checked_mul(Self::STEP_MS)
            .ok_or(TimeError::ArithmeticOverflow)?;
        Ok(Self {
            tick,
            calendar_epoch,
        })
    }
    pub const fn tick(self) -> u64 {
        self.tick
    }
    pub const fn simulation_ms(self) -> u64 {
        self.tick * Self::STEP_MS
    }
    pub const fn calendar_epoch(self) -> ClockRevision {
        self.calendar_epoch
    }
}

/// Current reason a server cannot start another requested step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SimulationLimit {
    None = 0,
    Batch = 1,
    CpuBudget = 2,
    Water = 3,
    Worker = 4,
    WorkerFailed = 5,
    UtcRange = 6,
}

impl SimulationLimit {
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Batch => "batch",
            Self::CpuBudget => "cpu_budget",
            Self::Water => "water",
            Self::Worker => "worker",
            Self::WorkerFailed => "worker_failed",
            Self::UtcRange => "utc_range",
        }
    }
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::None),
            1 => Some(Self::Batch),
            2 => Some(Self::CpuBudget),
            3 => Some(Self::Water),
            4 => Some(Self::Worker),
            5 => Some(Self::WorkerFailed),
            6 => Some(Self::UtcRange),
            _ => None,
        }
    }
}

/// Server sample is valid only in the process session named by its nonce.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockSample {
    achieved_ppm: u64,
    limit: SimulationLimit,
    boundary: SimulationBoundary,
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
    pub const fn with_progress(mut self, rate: u64, limit: SimulationLimit) -> Self {
        self.achieved_ppm = rate;
        self.limit = limit;
        self
    }
    pub const fn achieved_ppm(self) -> u64 {
        self.achieved_ppm
    }
    pub const fn limit(self) -> SimulationLimit {
        self.limit
    }

    pub const fn with_boundary(mut self, boundary: SimulationBoundary) -> Self {
        self.boundary = boundary;
        self
    }
    pub const fn boundary(self) -> SimulationBoundary {
        self.boundary
    }
    /// UTC belongs to the committed boundary. The monotonic timestamp identifies transmission time.
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
            achieved_ppm: 0,
            limit: SimulationLimit::None,
            boundary: SimulationBoundary::INITIAL,
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
    boundary: SimulationBoundary,
    clock_id: ClockId,
    revision: ClockRevision,
    mode: ClockMode,
    saved_world_utc_ms: WorldTimeMs,
    saved_wall_utc_ms: WorldTimeMs,
    rate_ppm: TimeScalePpm,
    offline_policy: OfflinePolicy,
}

impl SavedClockState {
    pub const fn with_boundary(mut self, boundary: SimulationBoundary) -> Self {
        self.boundary = boundary;
        self
    }
    pub const fn boundary(self) -> SimulationBoundary {
        self.boundary
    }
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
            boundary: SimulationBoundary::INITIAL,
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
