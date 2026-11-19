//! Immutable, per-instance owner capture; no solver, checkpoint or terrain work is started.
use super::*;
use minimium_procgen::SectorContentVersion;
use std::sync::atomic::Ordering;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaterStatusState {
    Ready,
    NotApplicable,
    Unavailable,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaterStatusError {
    UnknownInstance,
}

impl WaterStatusState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotApplicable => "not_applicable",
            Self::Unavailable => "unavailable",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum WaterJobState {
    WorkerPending,
    Ready,
}

impl WaterJobState {
    pub fn label(self) -> &'static str {
        match self {
            Self::WorkerPending => "worker_pending",
            Self::Ready => "ready",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WaterJobStatus {
    pub state: WaterJobState,
    pub target_tick: u64,
    pub input_terrain_version: SectorContentVersion,
    pub input_fluid_revision: u64,
}

#[derive(Clone, Debug)]
pub struct WaterStatus {
    pub instance: WorldInstanceId,
    pub capture_id: u64,
    pub committed_tick: u64,
    pub pending_target_tick: Option<u64>,
    pub state: WaterStatusState,
    pub reason: Option<&'static str>,
    pub fluid_revision: Option<u64>,
    pub persisted_revision: Option<u64>,
    pub dirty: Option<bool>,
    pub catalog_digest: Option<[u8; 32]>,
    pub terrain_version: Option<SectorContentVersion>,
    pub job: Option<WaterJobStatus>,
    pub failure: Option<&'static str>,
    pub retry_after_ms: Option<u64>,
    pub checkpoint_due_ms: Option<u64>,
    pub active_cells: Option<usize>,
    pub reserved_limit: Option<usize>,
}

impl ServerRuntime {
    pub(in crate::runtime) fn water_status_snapshot(
        &self,
        instance: WorldInstanceId,
    ) -> Option<WaterStatus> {
        let owner = self.game.worlds.get(instance)?;
        let capture_id = self
            .excavation
            .state
            .water_capture_sequence
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .unwrap_or(u64::MAX);
        let mut status = WaterStatus {
            instance,
            capture_id,
            committed_tick: self.environment_clock.boundary().tick(),
            pending_target_tick: self.scheduler.pending.map(|step| step.boundary.tick()),
            state: WaterStatusState::Ready,
            reason: None,
            fluid_revision: None,
            persisted_revision: None,
            dirty: None,
            catalog_digest: None,
            terrain_version: None,
            job: None,
            failure: None,
            retry_after_ms: None,
            checkpoint_due_ms: None,
            active_cells: None,
            reserved_limit: None,
        };
        if owner.manifest.planet().is_none() {
            status.state = WaterStatusState::NotApplicable;
            status.reason = Some("no_hydrology");
            return Some(status);
        }
        let state = &self.excavation.state;
        let Some(water) = state.waters.get(&instance) else {
            status.state = WaterStatusState::Unavailable;
            status.reason = Some("not_initialized");
            return Some(status);
        };
        status.fluid_revision = Some(water.snapshot().revision());
        status.persisted_revision = Some(water.persisted_revision());
        status.dirty = Some(water.dirty());
        status.active_cells = Some(water.snapshot().cells().len());
        status.reserved_limit = Some(water.reserved_cell_limit());
        status.catalog_digest = state.water_catalogs.get(&instance).copied();
        status.terrain_version = self
            .excavation_inputs(instance)
            .map(|inputs| inputs.revision());
        status.job = state
            .water_jobs
            .get(&instance)
            .map(hydrology::WaterJob::status);
        status.checkpoint_due_ms = state.water_checkpoint_due.get(&instance).map(|at| {
            at.saturating_duration_since(self.now)
                .as_millis()
                .min(u128::from(u64::MAX)) as u64
        });
        if let Some(failure) = state.water_failures.get(&instance) {
            status.state = WaterStatusState::Failed;
            status.failure = Some(fluid_error_code(failure.error));
            status.retry_after_ms = Some(failure.retry_after_ms(self.now));
        } else if self.excavation.coordinator.failure().is_some() {
            status.state = WaterStatusState::Failed;
            status.failure = Some("coordinator_failed");
        } else if status.catalog_digest.is_none() || status.terrain_version.is_none() {
            status.state = WaterStatusState::Unavailable;
            status.reason = Some("inputs_unavailable");
        }
        Some(status)
    }
}

fn fluid_error_code(error: minimium_fluids::FluidError) -> &'static str {
    use minimium_fluids::FluidError;
    match error {
        FluidError::Invalid => "invalid",
        FluidError::Capacity => "capacity",
        FluidError::Unavailable => "unavailable",
        FluidError::InsufficientVolume => "insufficient_volume",
        FluidError::DisplacementRequired => "displacement_required",
        FluidError::RevisionExhausted => "revision_exhausted",
    }
}
