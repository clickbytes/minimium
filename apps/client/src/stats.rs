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
    last_frame_at: Option<Instant>,
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
            last_frame_at: None,
        }
    }
}

impl FrameStats {
    pub(crate) fn report(&self) -> Option<minimium_commands::operation::CommandValue> {
        use minimium_commands::operation::{CommandValue as V, FiniteValue};
        use std::collections::BTreeMap;
        if self.sample_count == 0 || self.elapsed_nanoseconds == 0 {
            return None;
        }
        let mut fields = BTreeMap::new();
        let finite = |value: f64| V::Finite(FiniteValue::new(value).expect("finite frame metric"));
        fields.insert("sample_count".into(), V::U64(self.sample_count));
        fields.insert("presented_frames".into(), V::U64(self.presented_frames));
        if let Some(at) = self.last_frame_at {
            fields.insert(
                "sample_age_ms".into(),
                V::U64(at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
            );
        }
        fields.insert(
            "fps".into(),
            finite(self.presented_frames as f64 * 1e9 / self.elapsed_nanoseconds as f64),
        );
        if let Some(us) = self.frame_intervals.average_microseconds() {
            fields.insert("frame_average_ms".into(), finite(us as f64 / 1000.0));
        }
        if let Some(us) = self.cpu_work.average_microseconds() {
            fields.insert("cpu_average_ms".into(), finite(us as f64 / 1000.0));
        }
        for (name, percentile) in [
            ("frame_p50_ms", 50),
            ("frame_p95_ms", 95),
            ("frame_p99_ms", 99),
        ] {
            if let Some(us) = self.frame_intervals.percentile_microseconds(percentile) {
                fields.insert(name.into(), finite(us as f64 / 1000.0));
            }
        }
        for (name, percentile) in [("cpu_p50_ms", 50), ("cpu_p95_ms", 95), ("cpu_p99_ms", 99)] {
            if let Some(us) = self.cpu_work.percentile_microseconds(percentile) {
                fields.insert(name.into(), finite(us as f64 / 1000.0));
            }
        }
        if let Some(ms) = self.frame_intervals.maximum_milliseconds() {
            fields.insert("frame_max_ms".into(), finite(ms));
        }
        if let Some(ms) = self.cpu_work.maximum_milliseconds() {
            fields.insert("cpu_max_ms".into(), finite(ms));
        }
        if self.presented_frames > 0 {
            fields.insert(
                "draw_calls_per_presented_frame".into(),
                finite(self.total_draw_calls as f64 / self.presented_frames as f64),
            );
        }
        if let Some(cpu) = self.process_cpu_time {
            fields.insert(
                "process_cpu_time_ms".into(),
                V::U64(cpu.as_millis().min(u128::from(u64::MAX)) as u64),
            );
        }
        if let Some(bytes) = self.gpu_bytes {
            fields.insert("gpu_estimate_bytes".into(), V::U64(bytes));
        }
        fields.insert(
            "gpu_estimate_scope".into(),
            V::Enum("engine_resources_not_driver_vram".into()),
        );
        let environment = self.environment.snapshot();
        let histogram = |sample: environment::HistogramSnapshot| {
            let mut values = BTreeMap::new();
            values.insert("samples".into(), V::U64(sample.samples));
            for (name, value) in [("average_us", sample.average_us), ("max_us", sample.max_us)] {
                if let Some(value) = value.and_then(FiniteValue::new) {
                    values.insert(name.into(), V::Finite(value));
                }
            }
            for (name, value) in [
                ("p50_us", sample.p50_us),
                ("p95_us", sample.p95_us),
                ("p99_us", sample.p99_us),
            ] {
                if let Some(value) = value {
                    values.insert(name.into(), V::U64(value));
                }
            }
            V::Record(values)
        };
        fields.insert("environment_cpu".into(), histogram(environment.cpu));
        fields.insert(
            "environment_upload_cpu".into(),
            histogram(environment.upload_cpu),
        );
        fields.insert("shadow_cpu".into(), histogram(environment.shadow_cpu));
        for (name, value) in [
            ("ephemeris_evaluations", environment.ephemeris_evaluations),
            ("ephemeris_cache_hits", environment.ephemeris_cache_hits),
            ("star_epoch_updates", environment.star_epoch_updates),
            ("star_epoch_cache_hits", environment.star_epoch_cache_hits),
            ("sky_draw_calls", environment.sky_draw_calls),
            ("star_draw_calls", environment.star_draw_calls),
            ("sun_draw_calls", environment.sun_draw_calls),
            ("moon_draw_calls", environment.moon_draw_calls),
            ("halo_draw_calls", environment.halo_draw_calls),
            ("shadow_draw_calls", environment.shadow_draw_calls),
            ("environment_frames_skipped", environment.frames_skipped),
        ] {
            fields.insert(name.into(), V::U64(value));
        }
        Some(V::Record(fields))
    }

    /// Records one render-loop sample.
    ///
    /// `draw_calls == 0` denotes a skipped frame. Any positive value counts as
    /// a presented frame for FPS, so callers should pass zero when surface
    /// acquisition skips presentation.
    pub fn record_frame(&mut self, interval: Duration, cpu_work: Duration, draw_calls: u32) {
        self.last_frame_at = Some(Instant::now());
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

    pub(crate) fn reset_environment_epoch(&mut self) {
        self.environment.reset_epoch();
    }
}
