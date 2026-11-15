use minimium_simulation::CollisionQueryStats;
use minimium_telemetry::{
    allocations::{self, ENABLED, PHASES},
    duration::DurationStats,
    process::ProcessSampler,
};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(crate) struct RuntimeMetricsSnapshot {
    pub fields: Vec<MetricField>,
}
#[derive(Clone, Debug)]
pub(crate) struct MetricField {
    pub name: String,
    pub value: MetricValue,
}
#[derive(Clone, Debug)]
pub(crate) enum MetricValue {
    Count(u64),
    Optional(Option<u64>),
    Ratio(f64),
    Label(&'static str),
}

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

    pub(crate) fn record_component_candidates(&mut self, queued: usize, filtered: usize) {
        self.component_candidates_queued = self
            .component_candidates_queued
            .saturating_add(queued as u64);
        self.component_candidates_filtered = self
            .component_candidates_filtered
            .saturating_add(filtered as u64);
    }

    pub(crate) fn snapshot(&self, players: usize) -> RuntimeMetricsSnapshot {
        let mut fields = Vec::with_capacity(96);
        macro_rules! count {
            ($name:expr, $value:expr) => {
                fields.push(MetricField {
                    name: $name.to_owned(),
                    value: MetricValue::Count($value as u64),
                });
            };
        }
        macro_rules! optional {
            ($name:expr, $value:expr) => {
                fields.push(MetricField {
                    name: $name.to_owned(),
                    value: MetricValue::Optional($value),
                });
            };
        }
        count!("ticks", self.ticks);
        count!("missed_ticks", self.missed_ticks);
        count!("snapshots", self.snapshots);
        count!("missed_snapshots", self.missed_snapshots);
        count!("players", players);
        count!("bytes_in", self.bytes_in);
        count!("bytes_out", self.bytes_out);
        count!("decoded_frames", self.frames_in);
        count!("queued_frames", self.frames_out);
        count!("dropped_messages", self.dropped_messages);
        count!("dropped_snapshots", self.dropped_snapshots);
        count!("rejected_inputs", self.rejected_inputs);
        count!("clock_probe_requests", self.clock_probe_requests);
        count!("clock_probe_accepted", self.clock_probe_accepted);
        count!("clock_probe_rejected", self.clock_probe_rejected);
        count!("environment_sync_messages", self.environment_sync_messages);
        count!("idle_timeouts", self.idle_timeouts);
        count!("handshake_timeouts", self.handshake_timeouts);
        count!("shots_fired", self.shots_fired);
        count!("shots_hit", self.shots_hit);
        count!("player_kills", self.player_kills);
        optional!(
            "tick_last_us",
            (self.ticks > 0).then(|| self.last_tick.as_micros().min(u128::from(u64::MAX)) as u64)
        );
        for (prefix, stats) in [
            ("tick", &self.tick),
            ("loop", &self.loop_work),
            ("network", &self.network_io),
            ("input", &self.input_apply),
            ("simulation", &self.simulation),
            ("snapshot", &self.snapshot_prep),
            ("encoding", &self.encoding),
            ("input_wait", &self.input_wait),
        ] {
            optional!(&format!("{prefix}_avg_us"), stats.average_microseconds());
            optional!(
                &format!("{prefix}_p95_us"),
                stats.percentile_microseconds(95)
            );
            optional!(
                &format!("{prefix}_p99_us"),
                stats.percentile_microseconds(99)
            );
        }
        count!("oldest_output_age_us", self.oldest_output_age.as_micros());
        count!("collision_queries", self.collision_queries);
        count!("collision_cells", self.collision_cell_visits);
        fields.push(MetricField {
            name: "collision_cells_per_query".to_owned(),
            value: MetricValue::Ratio(
                self.collision_cell_visits as f64 / self.collision_queries.max(1) as f64,
            ),
        });
        count!(
            "collision_max_cells_per_query",
            self.collision_max_cell_visits
        );
        count!("collision_aabb_candidates", self.collision_aabb_candidates);
        fields.push(MetricField {
            name: "collision_aabb_candidates_per_query".to_owned(),
            value: MetricValue::Ratio(
                self.collision_aabb_candidates as f64 / self.collision_queries.max(1) as f64,
            ),
        });
        count!(
            "collision_max_aabb_candidates_per_query",
            self.collision_max_aabb_candidates
        );
        count!(
            "collision_triangle_candidates",
            self.collision_triangle_candidates
        );
        fields.push(MetricField {
            name: "collision_triangle_candidates_per_query".to_owned(),
            value: MetricValue::Ratio(
                self.collision_triangle_candidates as f64 / self.collision_queries.max(1) as f64,
            ),
        });
        count!(
            "collision_max_triangle_candidates_per_query",
            self.collision_max_triangle_candidates
        );
        count!(
            "collision_full_scan_queries",
            self.collision_fallback_queries
        );
        count!("precise_serial_steps", self.precise_serial_steps);
        count!("precise_batched_steps", self.precise_batched_steps);
        count!("precise_batches", self.precise_batches);
        count!("max_pending_inputs", self.max_pending_inputs);
        count!("max_client_input_bytes", self.max_client_input_bytes);
        count!("max_client_output_bytes", self.max_client_output_bytes);
        count!("memory_samples", self.memory_samples);
        optional!("memory_rss_bytes", self.memory_rss_bytes);
        optional!("memory_peak_rss_bytes", self.memory_peak_rss_bytes);
        optional!("memory_private_bytes", self.memory_private_bytes);
        optional!("memory_peak_private_bytes", self.memory_peak_private_bytes);
        optional!(
            "memory_rss_average_per_player_bytes",
            (players > 0)
                .then(|| self.memory_rss_bytes)
                .flatten()
                .map(|v| v / players as u64)
        );
        optional!(
            "process_cpu_time_ms",
            self.process_cpu_time
                .map(|v| v.as_millis().min(u128::from(u64::MAX)) as u64)
        );
        optional!("process_threads", self.process_threads.map(u64::from));
        count!(
            "component_candidates_queued",
            self.component_candidates_queued
        );
        count!(
            "component_candidates_filtered",
            self.component_candidates_filtered
        );
        if ENABLED {
            for (phase, counts) in PHASES.into_iter().zip(allocations::snapshot()) {
                count!(
                    format!("alloc_{}_allocations", phase.name()),
                    counts.allocations
                );
                count!(
                    format!("alloc_{}_reallocations", phase.name()),
                    counts.reallocations
                );
                count!(format!("alloc_{}_bytes", phase.name()), counts.bytes);
            }
            fields.push(MetricField {
                name: "allocation_metrics".to_owned(),
                value: MetricValue::Label("enabled"),
            });
        } else {
            fields.push(MetricField {
                name: "allocation_metrics".to_owned(),
                value: MetricValue::Label("disabled"),
            });
        }
        RuntimeMetricsSnapshot { fields }
    }
}
