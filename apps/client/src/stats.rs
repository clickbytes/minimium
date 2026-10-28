use minimium_telemetry::duration::DurationStats;
use minimium_telemetry::process::ProcessSampler;
use std::time::{Duration, Instant};

/// Fixed-cost client frame metrics. Recording a frame does not allocate.
#[derive(Default)]
pub struct FrameStats {
    frame_intervals: DurationStats,
    cpu_work: DurationStats,
    sample_count: u64,
    elapsed_nanoseconds: u128,
    presented_frames: u64,
    total_draw_calls: u64,
    gpu_bytes: Option<u64>,
    process: ProcessSampler,
    process_cpu_time: Option<Duration>,
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

    /// Formats the accumulated frame and CPU-work timing summaries.
    pub fn summary(&self) -> String {
        let elapsed_seconds = self.elapsed_nanoseconds as f64 / 1_000_000_000.0;
        let fps = if elapsed_seconds > 0.0 {
            self.presented_frames as f64 / elapsed_seconds
        } else {
            0.0
        };
        let draws_per_presented_frame = if self.presented_frames > 0 {
            self.total_draw_calls as f64 / self.presented_frames as f64
        } else {
            0.0
        };
        let frame_average_ms = micros_to_millis(self.frame_intervals.average_microseconds());
        let frame_p95_ms = micros_to_millis(self.frame_intervals.percentile_microseconds(95));
        let frame_p99_ms = micros_to_millis(self.frame_intervals.percentile_microseconds(99));
        let cpu_average_ms = micros_to_millis(self.cpu_work.average_microseconds());
        let cpu_p95_ms = micros_to_millis(self.cpu_work.percentile_microseconds(95));
        let cpu_p99_ms = micros_to_millis(self.cpu_work.percentile_microseconds(99));
        let gpu_estimate = self
            .gpu_bytes
            .map_or_else(|| "unavailable".to_string(), |bytes| format!("{bytes} B"));

        let process_cpu = self
            .process_cpu_time
            .map_or_else(|| "n/a".to_owned(), |time| time.as_millis().to_string());
        format!(
            "frames={} presented={} presented_FPS_avg={fps:.1} frame_ms(avg/p95/p99)={frame_average_ms:.3}/{frame_p95_ms:.3}/{frame_p99_ms:.3} cpu_work_ms(avg/p95/p99)={cpu_average_ms:.3}/{cpu_p95_ms:.3}/{cpu_p99_ms:.3} draw_calls_per_presented_frame={draws_per_presented_frame:.2} process_cpu_time_ms={process_cpu} engine_GPU_estimate={gpu_estimate}",
            self.sample_count, self.presented_frames
        )
    }
}

fn micros_to_millis(microseconds: u64) -> f64 {
    microseconds as f64 / 1_000.0
}
