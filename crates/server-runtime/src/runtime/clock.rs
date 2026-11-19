use super::*;
use minimium_environment::time::SimulationLimit;

impl ServerRuntime {
    pub(super) fn monotonic_ms(&self, now: Instant) -> io::Result<MonotonicMs> {
        let elapsed = now.saturating_duration_since(self.scheduler.server_started);
        let millis = u64::try_from(elapsed.as_millis()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "server monotonic time overflow")
        })?;
        Ok(MonotonicMs::new(millis))
    }

    pub(super) fn next_wake(&self) -> Instant {
        let poll_interval = if self.connections.clients.is_empty() {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(8)
        };
        let now = self.now;
        let until_tick = self.scheduler.demand.until_due(
            self.environment_clock.rate().get(),
            self.simulation_work_pending()
                || self.environment_clock.limit == SimulationLimit::Water,
        );
        let until_snapshot = if self.scheduler.pending.is_some() {
            poll_interval
        } else {
            self.scheduler.next_snapshot.saturating_duration_since(now)
        };
        let until_autosave = self
            .scheduler
            .next_autosave
            .map(|deadline| deadline.saturating_duration_since(now))
            .unwrap_or(Duration::MAX);
        now + poll_interval.min(until_tick).min(until_snapshot).min(
            if self.simulation_work_pending() {
                poll_interval
            } else {
                until_autosave
            },
        )
    }
}

#[derive(Clone, Copy)]
pub(super) struct PendingStep {
    pub(super) boundary: minimium_environment::time::SimulationBoundary,
    pub(super) utc: minimium_environment::time::WorldTimeMs,
    pub(super) operation: usize,
}

impl ServerRuntime {
    pub(super) fn step_boundary(&self) -> minimium_environment::time::SimulationBoundary {
        self.scheduler
            .pending
            .map_or(self.environment_clock.boundary(), |step| step.boundary)
    }
}
const NORMAL_BATCH: u64 = 8;
const FAST_FORWARD_BATCH: u64 = 1024;
const STEP_UNITS: u128 = TICK.as_nanos() * 1_000_000;

pub(super) struct StepDemand {
    last: Instant,
    units: u128,
    rate: u64,
    window_start: Instant,
    completed: u64,
}

impl StepDemand {
    pub(super) fn new(now: Instant) -> Self {
        Self {
            last: now,
            units: 0,
            rate: 0,
            window_start: now,
            completed: 0,
        }
    }
    pub(super) fn batch_limit(&self) -> u64 {
        // Keep at most one ordinary tick period of accelerated demand.
        self.rate
            .div_ceil(minimium_environment::time::ONE_X_PPM)
            .clamp(NORMAL_BATCH, FAST_FORWARD_BATCH)
    }
    pub(super) fn accrue(&mut self, now: Instant, rate: u64) -> u64 {
        let elapsed = now.duration_since(self.last);
        self.last = now;
        self.rate = rate;
        self.units = self
            .units
            .saturating_add(elapsed.as_nanos().saturating_mul(u128::from(rate)));
        let due = self.units / STEP_UNITS;
        let excess = due.saturating_sub(u128::from(self.batch_limit()));
        self.units -= excess * STEP_UNITS;
        u64::try_from(excess).unwrap_or(u64::MAX)
    }
    pub(super) fn control_changed(&mut self, rate: u64) {
        if self.rate != rate {
            self.units = 0;
            self.rate = rate;
        }
    }
    pub(super) fn due(&self) -> bool {
        self.units >= STEP_UNITS
    }
    pub(super) fn consume(&mut self) {
        self.units -= STEP_UNITS;
    }
    pub(super) fn finish(&mut self) {
        self.completed += 1;
    }
    pub(super) fn measured_rate(&mut self, now: Instant) -> Option<u64> {
        let nanos = now.duration_since(self.window_start).as_nanos();
        if nanos < 1_000_000_000 {
            return None;
        }
        self.window_start = now;
        Some((u128::from(std::mem::take(&mut self.completed)) * STEP_UNITS / nanos) as u64)
    }
    pub(super) fn until_due(&self, rate: u64, waiting: bool) -> Duration {
        if rate == 0 {
            return Duration::MAX;
        }
        if waiting {
            return Duration::from_millis(1);
        }
        Duration::from_nanos(
            STEP_UNITS
                .saturating_sub(self.units)
                .div_ceil(u128::from(rate)) as u64,
        )
    }
}
