use minimium_core::world::CollisionQueryStats;
use minimium_telemetry::{
    allocations::{self, ENABLED, PHASES},
    duration::DurationStats,
    process::ProcessSampler,
};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct RuntimeMetrics {
    pub(crate) ticks: u64,
    pub(crate) missed_ticks: u64,
    pub(crate) snapshots: u64,
    pub(crate) missed_snapshots: u64,
    pub(crate) bytes_in: u64,
    pub(crate) bytes_out: u64,
    pub(crate) frames_in: u64,
    pub(crate) frames_out: u64,
    pub(crate) dropped_messages: u64,
    pub(crate) dropped_snapshots: u64,
    pub(crate) rejected_inputs: u64,
    pub(crate) clock_probe_requests: u64,
    pub(crate) clock_probe_accepted: u64,
    pub(crate) clock_probe_rejected: u64,
    pub(crate) environment_sync_messages: u64,
    pub(crate) idle_timeouts: u64,
    pub(crate) handshake_timeouts: u64,
    pub(crate) shots_fired: u64,
    pub(crate) shots_hit: u64,
    pub(crate) player_kills: u64,
    pub(crate) last_tick: Duration,
    pub(crate) tick: DurationStats,
    pub(crate) loop_work: DurationStats,
    pub(crate) network_io: DurationStats,
    pub(crate) input_apply: DurationStats,
    pub(crate) simulation: DurationStats,
    pub(crate) snapshot_prep: DurationStats,
    pub(crate) encoding: DurationStats,
    pub(crate) input_wait: DurationStats,
    pub(crate) oldest_output_age: Duration,
    pub(crate) collision_queries: u64,
    pub(crate) collision_cell_visits: u64,
    pub(crate) collision_aabb_candidates: u64,
    pub(crate) collision_triangle_candidates: u64,
    pub(crate) collision_fallback_queries: u64,
    pub(crate) collision_max_cell_visits: u64,
    pub(crate) collision_max_aabb_candidates: u64,
    pub(crate) collision_max_triangle_candidates: u64,
    pub(crate) precise_serial_steps: u64,
    pub(crate) precise_batched_steps: u64,
    pub(crate) precise_batches: u64,
    pub(crate) component_candidates_queued: u64,
    pub(crate) component_candidates_filtered: u64,
    pub(crate) memory_sampled_at: Option<Instant>,
    pub(crate) memory_samples: u64,
    pub(crate) memory_rss_bytes: Option<u64>,
    pub(crate) memory_peak_rss_bytes: Option<u64>,
    pub(crate) memory_private_bytes: Option<u64>,
    pub(crate) memory_peak_private_bytes: Option<u64>,
    pub(crate) process_cpu_time: Option<Duration>,
    pub(crate) process_threads: Option<u32>,
    process_sampler: ProcessSampler,
    pub(crate) max_pending_inputs: usize,
    pub(crate) max_client_input_bytes: usize,
    pub(crate) max_client_output_bytes: usize,
}

impl RuntimeMetrics {
    pub(crate) fn sample_process_memory(&mut self, now: Instant) {
        if self.memory_sampled_at.is_some_and(|sampled_at| {
            now.saturating_duration_since(sampled_at) < Duration::from_secs(1)
        }) {
            return;
        }
        self.memory_sampled_at = Some(now);
        let sample = self.process_sampler.sample(now);
        self.process_cpu_time = sample.cpu_time;
        self.process_threads = sample.threads;
        self.memory_rss_bytes = sample.rss_bytes;
        self.memory_private_bytes = sample.private_bytes;
        if let Some(rss_bytes) = sample.rss_bytes {
            self.memory_samples = self.memory_samples.saturating_add(1);
            self.memory_rss_bytes = Some(rss_bytes);
            self.memory_peak_rss_bytes = Some(
                self.memory_peak_rss_bytes
                    .unwrap_or_default()
                    .max(rss_bytes),
            );
        }
        if let Some(private_bytes) = sample.private_bytes {
            self.memory_private_bytes = Some(private_bytes);
            self.memory_peak_private_bytes = Some(
                self.memory_peak_private_bytes
                    .unwrap_or_default()
                    .max(private_bytes),
            );
        }
    }

    pub(crate) fn record_collision_queries(&mut self, stats: CollisionQueryStats) {
        self.collision_queries = self.collision_queries.saturating_add(stats.queries);
        self.collision_cell_visits = self.collision_cell_visits.saturating_add(stats.cell_visits);
        self.collision_aabb_candidates = self
            .collision_aabb_candidates
            .saturating_add(stats.aabb_candidates);
        self.collision_triangle_candidates = self
            .collision_triangle_candidates
            .saturating_add(stats.triangle_candidates);
        self.collision_fallback_queries = self
            .collision_fallback_queries
            .saturating_add(stats.fallback_queries);
        self.collision_max_cell_visits = self.collision_max_cell_visits.max(stats.max_cell_visits);
        self.collision_max_aabb_candidates = self
            .collision_max_aabb_candidates
            .max(stats.max_aabb_candidates);
        self.collision_max_triangle_candidates = self
            .collision_max_triangle_candidates
            .max(stats.max_triangle_candidates);
    }

    pub(crate) fn record_precise_dispatch(&mut self, batched: bool, batches: usize) {
        if batched {
            self.precise_batched_steps = self.precise_batched_steps.saturating_add(1);
            self.precise_batches = self.precise_batches.saturating_add(batches as u64);
        } else {
            self.precise_serial_steps = self.precise_serial_steps.saturating_add(1);
        }
    }

    #[expect(
        dead_code,
        reason = "component candidate metrics activate with replication opt-in"
    )]
    pub(crate) fn record_component_candidates(&mut self, queued: usize, filtered: usize) {
        self.component_candidates_queued = self
            .component_candidates_queued
            .saturating_add(queued as u64);
        self.component_candidates_filtered = self
            .component_candidates_filtered
            .saturating_add(filtered as u64);
    }

    pub(crate) fn summary(&self, players: usize) -> String {
        let collision_query_count = self.collision_queries.max(1) as f64;
        let rss_average_per_player = (players > 0)
            .then(|| self.memory_rss_bytes.map(|bytes| bytes / players as u64))
            .flatten();
        let mut summary = format!(
            "ticks={} missed_ticks={} snapshots={} missed_snapshots={} players={} bytes_in={} bytes_out={} decoded_frames={} queued_frames={} dropped_messages={} dropped_snapshots={} rejected_inputs={} clock_probe_requests={} clock_probe_accepted={} clock_probe_rejected={} environment_sync_messages={} idle_timeouts={} handshake_timeouts={} shots_fired={} shots_hit={} player_kills={} tick_last_us={} tick_avg_us={} tick_p95_us={} tick_p99_us={} loop_avg_us={} loop_p95_us={} loop_p99_us={} network_avg_us={} network_p95_us={} network_p99_us={} input_avg_us={} input_p95_us={} input_p99_us={} simulation_avg_us={} simulation_p95_us={} simulation_p99_us={} snapshot_avg_us={} snapshot_p95_us={} snapshot_p99_us={} encoding_avg_us={} encoding_p95_us={} encoding_p99_us={} input_wait_avg_us={} input_wait_p95_us={} input_wait_p99_us={} oldest_output_age_us={} collision_queries={} collision_cells={} collision_cells_per_query={:.2} collision_max_cells_per_query={} collision_aabb_candidates={} collision_aabb_candidates_per_query={:.2} collision_max_aabb_candidates_per_query={} collision_triangle_candidates={} collision_triangle_candidates_per_query={:.2} collision_max_triangle_candidates_per_query={} collision_full_scan_queries={} precise_serial_steps={} precise_batched_steps={} precise_batches={} max_pending_inputs={} max_client_input_bytes={} max_client_output_bytes={} memory_samples={} memory_rss_bytes={} memory_peak_rss_bytes={} memory_private_bytes={} memory_peak_private_bytes={} memory_rss_average_per_player_bytes={} process_cpu_time_ms={} process_threads={}",
            self.ticks, self.missed_ticks, self.snapshots, self.missed_snapshots, players,
            self.bytes_in, self.bytes_out, self.frames_in, self.frames_out,
            self.dropped_messages, self.dropped_snapshots, self.rejected_inputs,
            self.clock_probe_requests, self.clock_probe_accepted,
            self.clock_probe_rejected, self.environment_sync_messages,
            self.idle_timeouts, self.handshake_timeouts, self.shots_fired, self.shots_hit,
            self.player_kills, format_duration_us((self.ticks > 0).then(|| self.last_tick.as_micros().min(u128::from(u64::MAX)) as u64)),
            format_duration_us(self.tick.average_microseconds()), format_duration_us(self.tick.percentile_microseconds(95)),
            format_duration_us(self.tick.percentile_microseconds(99)),
            format_duration_us(self.loop_work.average_microseconds()), format_duration_us(self.loop_work.percentile_microseconds(95)),
            format_duration_us(self.loop_work.percentile_microseconds(99)),
            format_duration_us(self.network_io.average_microseconds()), format_duration_us(self.network_io.percentile_microseconds(95)),
            format_duration_us(self.network_io.percentile_microseconds(99)),
            format_duration_us(self.input_apply.average_microseconds()), format_duration_us(self.input_apply.percentile_microseconds(95)),
            format_duration_us(self.input_apply.percentile_microseconds(99)),
            format_duration_us(self.simulation.average_microseconds()), format_duration_us(self.simulation.percentile_microseconds(95)),
            format_duration_us(self.simulation.percentile_microseconds(99)),
            format_duration_us(self.snapshot_prep.average_microseconds()), format_duration_us(self.snapshot_prep.percentile_microseconds(95)),
            format_duration_us(self.snapshot_prep.percentile_microseconds(99)),
            format_duration_us(self.encoding.average_microseconds()), format_duration_us(self.encoding.percentile_microseconds(95)),
            format_duration_us(self.encoding.percentile_microseconds(99)),
            format_duration_us(self.input_wait.average_microseconds()), format_duration_us(self.input_wait.percentile_microseconds(95)),
            format_duration_us(self.input_wait.percentile_microseconds(99)), self.oldest_output_age.as_micros(),
            self.collision_queries, self.collision_cell_visits,
            self.collision_cell_visits as f64 / collision_query_count,
            self.collision_max_cell_visits, self.collision_aabb_candidates,
            self.collision_aabb_candidates as f64 / collision_query_count,
            self.collision_max_aabb_candidates, self.collision_triangle_candidates,
            self.collision_triangle_candidates as f64 / collision_query_count,
            self.collision_max_triangle_candidates, self.collision_fallback_queries,
            self.precise_serial_steps, self.precise_batched_steps, self.precise_batches,
            self.max_pending_inputs, self.max_client_input_bytes, self.max_client_output_bytes,
            self.memory_samples, format_optional_bytes(self.memory_rss_bytes),
            format_optional_bytes(self.memory_peak_rss_bytes),
            format_optional_bytes(self.memory_private_bytes),
            format_optional_bytes(self.memory_peak_private_bytes),
            format_optional_bytes(rss_average_per_player),
            self.process_cpu_time.map_or_else(|| "n/a".to_owned(), |time| time.as_millis().to_string()),
            self.process_threads.map_or_else(|| "n/a".to_owned(), |threads| threads.to_string()),
        );
        summary.push_str(&format!(
            " component_candidates_queued={} component_candidates_filtered={}",
            self.component_candidates_queued, self.component_candidates_filtered
        ));
        if ENABLED {
            for (phase, counts) in PHASES.into_iter().zip(allocations::snapshot()) {
                summary.push_str(&format!(
                    " alloc_{}_allocations={} alloc_{}_reallocations={} alloc_{}_bytes={}",
                    phase.name(),
                    counts.allocations,
                    phase.name(),
                    counts.reallocations,
                    phase.name(),
                    counts.bytes
                ));
            }
            summary.push_str(" allocation_metrics=enabled");
        } else {
            summary.push_str(" allocation_metrics=disabled");
        }
        summary
    }
}

fn format_optional_bytes(value: Option<u64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |bytes| bytes.to_string())
}

fn format_duration_us(value: Option<u64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |duration| duration.to_string())
}
