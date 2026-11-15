use super::*;
use minimium_telemetry::duration::DurationStats;

#[derive(Clone, Debug)]
pub(crate) struct ExcavationStatus {
    pub chunks: usize,
    pub pending: usize,
    pub bytes: u64,
    pub ready: bool,
    pub fluid_cells: usize,
    pub fluid_jobs: usize,
    pub terrain_payload_bytes: usize,
    pub fluid_payload_bytes: usize,
    pub fluid_reserved: usize,
    pub failed_worlds: usize,
    pub percentiles: Vec<(&'static str, Option<u64>, Option<u64>)>,
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
