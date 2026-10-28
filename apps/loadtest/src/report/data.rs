use super::super::{ClientMeasurement, WorkerStats};
use super::ReportMetadata;

/// Stable view of run inputs shared by the output format implementations.
pub(super) struct ReportData<'a> {
    pub scenario: &'a str,
    pub requested_clients: usize,
    pub workers: usize,
    pub elapsed_seconds: f64,
    pub elapsed_rate_denominator: f64,
    pub stats: &'a WorkerStats,
    pub metadata: &'a ReportMetadata,
}

impl<'a> ReportData<'a> {
    pub fn new(
        scenario: &'a str,
        requested_clients: usize,
        workers: usize,
        elapsed_seconds: f64,
        stats: &'a WorkerStats,
        metadata: &'a ReportMetadata,
    ) -> Self {
        Self {
            scenario,
            requested_clients,
            workers,
            elapsed_seconds,
            elapsed_rate_denominator: elapsed_seconds.max(0.001),
            stats,
            metadata,
        }
    }

    pub fn client_measurements(&self) -> &[ClientMeasurement] {
        &self.stats.per_client
    }
}
