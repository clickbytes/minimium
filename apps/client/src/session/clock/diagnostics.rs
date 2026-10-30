use super::ClientClock;
use crate::environment::{ClockDiagnostics, ClockSource};
use minimium_core::time::ClockSample;

pub(super) fn clock_diagnostics(
    clock: &ClientClock,
    sample: ClockSample,
    now_ms: u64,
) -> ClockDiagnostics {
    let probe = clock.best_offset();
    ClockDiagnostics {
        source: ClockSource::Server,
        clock_id: Some(sample.clock_id()),
        mode: Some(sample.mode()),
        offline_policy: Some(sample.offline_policy()),
        sample_age_real_ms: Some(
            now_ms.saturating_sub(clock.sample_received_at_ms.unwrap_or(now_ms)),
        ),
        clock_offset_ms: probe.map(|value| value.offset_twice_ms as f64 / 2.0),
        probe_rtt_ms: probe.map(|value| value.rtt_ms),
        samples_accepted: Some(clock.accepted_probe_count),
        samples_rejected: Some(clock.rejected_probe_count),
    }
}
