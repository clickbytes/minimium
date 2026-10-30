use minimium_telemetry::duration::DurationStats;
use minimium_telemetry::process::ProcessSampler;
use std::time::{Duration, Instant};

pub(crate) mod environment;

/// Fixed-storage frame histograms; periodic process sampling may allocate on
/// platforms that read process data from files.
pub struct FrameStats {
    frame_intervals: DurationStats,
    cpu_work: DurationStats,
    sample_count: u64,
    elapsed_nanoseconds: u128,
    presented_frames: u64,
    total_draw_calls: u64,
    gpu_bytes: Option<u64>,
    environment: environment::EnvironmentStats,
    process: ProcessSampler,
    process_cpu_time: Option<Duration>,
}

impl Default for FrameStats {
    fn default() -> Self {
        Self {
            frame_intervals: DurationStats::frames(),
            cpu_work: DurationStats::frames(),
            sample_count: 0,
            elapsed_nanoseconds: 0,
            presented_frames: 0,
            total_draw_calls: 0,
            gpu_bytes: None,
            environment: environment::EnvironmentStats::default(),
            process: ProcessSampler::default(),
            process_cpu_time: None,
        }
    }
}

impl FrameStats {
    /// Records one render-loop sample.
    ///
    /// `draw_calls == 0` denotes a skipped frame. Any positive value counts as
    /// a presented frame for FPS, so callers should pass zero when surface
    /// acquisition skips presentation.
    pub fn record_frame(&mut self, interval: Duration, cpu_work: Duration, draw_calls: u32) {
        self.process_cpu_time = self.process.sample(Instant::now()).cpu_time;
        self.frame_intervals.record(interval);
        self.cpu_work.record(cpu_work);
        self.sample_count = self.sample_count.saturating_add(1);
        self.elapsed_nanoseconds = self.elapsed_nanoseconds.saturating_add(interval.as_nanos());
        self.total_draw_calls = self.total_draw_calls.saturating_add(u64::from(draw_calls));
        if draw_calls > 0 {
            self.presented_frames = self.presented_frames.saturating_add(1);
        }
    }

    /// Sets the renderer's current engine-side GPU memory estimate.
    pub fn set_gpu_bytes(&mut self, bytes: u64) {
        self.gpu_bytes = Some(bytes);
    }

    pub(crate) fn record_environment_cpu(&mut self, duration: Duration) {
        self.environment.record_cpu(duration);
    }

    pub(crate) fn record_shadow_cpu(&mut self, duration: Duration) {
        self.environment.record_shadow_cpu(duration);
    }

    pub(crate) fn record_environment_upload_cpu(&mut self, duration: Duration) {
        self.environment.record_upload_cpu(duration);
    }

    pub(crate) fn set_astronomy_counters(
        &mut self,
        ephemeris_evaluations: u64,
        ephemeris_cache_hits: u64,
        star_epoch_updates: u64,
        star_epoch_cache_hits: u64,
    ) {
        self.environment.set_astronomy_counters(
            ephemeris_evaluations,
            ephemeris_cache_hits,
            star_epoch_updates,
            star_epoch_cache_hits,
        );
    }

    pub(crate) fn record_environment_frame(
        &mut self,
        presented: bool,
        draws: environment::EnvironmentDrawCounts,
    ) {
        self.environment.record_frame(presented, draws);
    }

    pub(crate) fn environment_snapshot(&self) -> environment::EnvironmentStatsSnapshot {
        self.environment.snapshot()
    }

    pub(crate) fn reset_environment_epoch(&mut self) {
        self.environment.reset_epoch();
    }

    /// Formats the accumulated frame and CPU-work timing summaries.
    pub fn summary(&self) -> String {
        let elapsed_seconds = self.elapsed_nanoseconds as f64 / 1_000_000_000.0;
        let fps = if self.sample_count > 0 && elapsed_seconds > 0.0 {
            format!("{:.1}", self.presented_frames as f64 / elapsed_seconds)
        } else {
            "n/a".to_owned()
        };
        let draws_per_presented_frame = if self.presented_frames > 0 {
            format!(
                "{:.2}",
                self.total_draw_calls as f64 / self.presented_frames as f64
            )
        } else {
            "n/a".to_owned()
        };
        let frame_average_ms = format_duration_ms(self.frame_intervals.average_microseconds());
        let frame_p50_ms = format_duration_ms(self.frame_intervals.percentile_microseconds(50));
        let frame_p95_ms = format_duration_ms(self.frame_intervals.percentile_microseconds(95));
        let frame_p99_ms = format_duration_ms(self.frame_intervals.percentile_microseconds(99));
        let frame_max_ms = format_milliseconds(self.frame_intervals.maximum_milliseconds());
        let cpu_average_ms = format_duration_ms(self.cpu_work.average_microseconds());
        let cpu_p50_ms = format_duration_ms(self.cpu_work.percentile_microseconds(50));
        let cpu_p95_ms = format_duration_ms(self.cpu_work.percentile_microseconds(95));
        let cpu_p99_ms = format_duration_ms(self.cpu_work.percentile_microseconds(99));
        let cpu_max_ms = format_milliseconds(self.cpu_work.maximum_milliseconds());
        let gpu_estimate = self
            .gpu_bytes
            .map_or_else(|| "unavailable".to_string(), |bytes| format!("{bytes} B"));

        let process_cpu = self
            .process_cpu_time
            .map_or_else(|| "n/a".to_owned(), |time| time.as_millis().to_string());
        format!(
            "frames={} presented={} presented_FPS_avg={fps} frame_ms(avg/p50/p95/p99/max)={frame_average_ms}/{frame_p50_ms}/{frame_p95_ms}/{frame_p99_ms}/{frame_max_ms} cpu_work_ms(avg/p50/p95/p99/max)={cpu_average_ms}/{cpu_p50_ms}/{cpu_p95_ms}/{cpu_p99_ms}/{cpu_max_ms} timing_samples={} draw_calls_per_presented_frame={draws_per_presented_frame} process_cpu_time_ms={process_cpu} engine_GPU_estimate={gpu_estimate}",
            self.sample_count, self.presented_frames, self.sample_count
        )
    }
}

fn format_milliseconds(milliseconds: Option<f64>) -> String {
    milliseconds.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.3}"))
}

fn format_duration_ms(microseconds: Option<u64>) -> String {
    microseconds.map_or_else(
        || "n/a".to_owned(),
        |value| format!("{:.3}", value as f64 / 1_000.0),
    )
}
