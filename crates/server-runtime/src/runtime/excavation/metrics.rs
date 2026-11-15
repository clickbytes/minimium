use super::*;
use minimium_telemetry::duration::DurationStats;

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
    pub(in crate::runtime) fn excavation_status(&self) -> String {
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
        let metrics = &self.excavation.metrics;
        let fluid_cells: usize = self
            .excavation
            .waters
            .values()
            .map(|water| water.snapshot().cells().len())
            .sum();
        let terrain_payload_bytes = chunks
            * (minimium_world::terrain::TERRAIN_CHUNK_CELLS as usize).pow(3)
            * std::mem::size_of::<minimium_world::terrain::TerrainSample>();
        let fluid_payload_bytes = fluid_cells
            * (std::mem::size_of::<minimium_fluids::CellWater>() + std::mem::size_of::<[i32; 3]>());
        let fluid_reserved: usize = self
            .excavation
            .waters
            .values()
            .map(|water| water.reserved_cell_limit())
            .sum();
        let mut result = format!(
            " excavation_chunks={chunks} excavation_pending={} excavation_bytes={} excavation_storage={} fluid_cells={fluid_cells} fluid_jobs={} terrain_payload_bytes={terrain_payload_bytes} fluid_payload_bytes={fluid_payload_bytes}",
            usize::from(self.excavation.busy()),
            metrics.bytes,
            if self.excavation.failure.is_some() {
                "failed"
            } else {
                "ready"
            },
            self.excavation.water_jobs.len(),
        );
        use std::fmt::Write;
        let _ = write!(
            result,
            " fluid_reserved={fluid_reserved} fluid_failed_worlds={}",
            self.excavation.water_failures.len()
        );
        for (name, stats) in [
            ("prepare", &metrics.prepare),
            ("collision", &metrics.collision),
            ("solver", &metrics.solver),
            ("persist", &metrics.persistence),
            ("publish", &metrics.publication),
        ] {
            let _ = write!(
                result,
                " excavation_{name}_p95_us={:?} excavation_{name}_p99_us={:?}",
                stats.percentile_microseconds(95),
                stats.percentile_microseconds(99),
            );
        }
        result
    }
}
