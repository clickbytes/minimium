use super::*;
use minimium_telemetry::duration::DurationStats;

#[derive(Clone, Debug)]
pub struct ExcavationStatus {
    pub(crate) chunks: usize,
    pub(crate) pending: usize,
    pub(crate) bytes: u64,
    pub(crate) ready: bool,
    pub(crate) fluid_cells: usize,
    pub(crate) fluid_jobs: usize,
    pub(crate) terrain_payload_bytes: usize,
    pub(crate) fluid_payload_bytes: usize,
    pub(crate) fluid_reserved: usize,
    pub(crate) failed_worlds: usize,
    pub(crate) percentiles: Vec<(&'static str, Option<u64>, Option<u64>)>,
}
impl ExcavationStatus {
    pub fn chunks(&self) -> usize {
        self.chunks
    }
    pub fn pending(&self) -> usize {
        self.pending
    }
    pub fn bytes(&self) -> u64 {
        self.bytes
    }
    pub fn ready(&self) -> bool {
        self.ready
    }
    pub fn fluid_cells(&self) -> usize {
        self.fluid_cells
    }
    pub fn fluid_jobs(&self) -> usize {
        self.fluid_jobs
    }
    pub fn terrain_payload_bytes(&self) -> usize {
        self.terrain_payload_bytes
    }
    pub fn fluid_payload_bytes(&self) -> usize {
        self.fluid_payload_bytes
    }
    pub fn fluid_reserved(&self) -> usize {
        self.fluid_reserved
    }
    pub fn failed_worlds(&self) -> usize {
        self.failed_worlds
    }
    pub fn percentiles(&self) -> &[(&'static str, Option<u64>, Option<u64>)] {
        &self.percentiles
    }
}

#[derive(Default)]
pub(super) struct ExcavationMetrics {
    pub prepare: DurationStats,
    pub collision: DurationStats,
    pub solver: DurationStats,
    pub persistence: DurationStats,
    pub publication: DurationStats,
    pub bytes: u64,
}

impl ServerRuntime {
    pub(in crate::runtime) fn excavation_status_snapshot(&self) -> ExcavationStatus {
        let chunks: usize = self
            .game
            .worlds
            .iter()
            .map(|(_, owner)| {
                owner
                    .volumes
                    .as_ref()
                    .map_or(0, |store| store.chunks().count())
            })
            .sum();
        let metrics = &self.excavation.state.metrics;
        let fluid_cells: usize = self
            .excavation
            .state
            .waters
            .values()
            .map(|water| water.snapshot().cells().len())
            .sum();
        let terrain_payload_bytes = chunks
            * (minimium_world::terrain::TERRAIN_CHUNK_CELLS as usize).pow(3)
            * std::mem::size_of::<minimium_world::terrain::TerrainSample>();
        let fluid_payload_bytes = fluid_cells
            * (std::mem::size_of::<minimium_fluids::CellWater>() + std::mem::size_of::<[i32; 3]>());
        let fluid_reserved = self
            .excavation
            .state
            .waters
            .values()
            .map(|water| water.reserved_cell_limit())
            .sum();
        ExcavationStatus {
            chunks,
            pending: usize::from(self.excavation.busy()),
            bytes: metrics.bytes,
            ready: self.excavation.coordinator.failure().is_none(),
            fluid_cells,
            fluid_jobs: self.excavation.state.water_jobs.len(),
            terrain_payload_bytes,
            fluid_payload_bytes,
            fluid_reserved,
            failed_worlds: self.excavation.state.water_failures.len(),
            percentiles: [
                (&metrics.prepare),
                (&metrics.collision),
                (&metrics.solver),
                (&metrics.persistence),
                (&metrics.publication),
            ]
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                (
                    ["prepare", "collision", "solver", "persist", "publish"][i],
                    s.percentile_microseconds(95),
                    s.percentile_microseconds(99),
                )
            })
            .collect(),
        }
    }
}
