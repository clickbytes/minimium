use std::time::Duration;

const ENVIRONMENT_BOUNDS_US: [u64; 13] = [
    1, 2, 5, 10, 20, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000,
];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct HistogramSnapshot {
    pub(crate) samples: u64,
    pub(crate) average_us: Option<f64>,
    pub(crate) p50_us: Option<u64>,
    pub(crate) p95_us: Option<u64>,
    pub(crate) p99_us: Option<u64>,
    pub(crate) max_us: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
struct MicrosecondHistogram {
    samples: u64,
    total_nanoseconds: u128,
    maximum_nanoseconds: u64,
    buckets: [u64; 14],
}

impl MicrosecondHistogram {
    fn record(&mut self, duration: Duration) {
        let nanos = duration.as_nanos().min(u128::from(u64::MAX)) as u64;
        let bucket = ENVIRONMENT_BOUNDS_US
            .iter()
            .position(|bound| nanos <= bound.saturating_mul(1_000))
            .unwrap_or(ENVIRONMENT_BOUNDS_US.len());
        self.samples = self.samples.saturating_add(1);
        self.total_nanoseconds = self.total_nanoseconds.saturating_add(u128::from(nanos));
        self.maximum_nanoseconds = self.maximum_nanoseconds.max(nanos);
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
    }

    fn snapshot(self) -> HistogramSnapshot {
        if self.samples == 0 {
            return HistogramSnapshot::default();
        }
        HistogramSnapshot {
            samples: self.samples,
            average_us: Some(self.total_nanoseconds as f64 / self.samples as f64 / 1_000.0),
            p50_us: self.percentile(50),
            p95_us: self.percentile(95),
            p99_us: self.percentile(99),
            max_us: Some(self.maximum_nanoseconds as f64 / 1_000.0),
        }
    }

    fn percentile(self, percentile: u64) -> Option<u64> {
        if self.samples == 0 {
            return None;
        }
        let rank = u128::from(self.samples)
            .saturating_mul(u128::from(percentile))
            .div_ceil(100) as u64;
        let mut cumulative = 0_u64;
        for (index, count) in self.buckets.iter().enumerate() {
            cumulative = cumulative.saturating_add(*count);
            if cumulative >= rank {
                return Some(
                    ENVIRONMENT_BOUNDS_US
                        .get(index)
                        .copied()
                        .unwrap_or(self.maximum_nanoseconds / 1_000),
                );
            }
        }
        Some(self.maximum_nanoseconds / 1_000)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct EnvironmentStatsSnapshot {
    pub(crate) cpu: HistogramSnapshot,
    pub(crate) upload_cpu: HistogramSnapshot,
    pub(crate) shadow_cpu: HistogramSnapshot,
    pub(crate) ephemeris_evaluations: u64,
    pub(crate) ephemeris_cache_hits: u64,
    pub(crate) star_epoch_updates: u64,
    pub(crate) star_epoch_cache_hits: u64,
    pub(crate) sky_draw_calls: u64,
    pub(crate) star_draw_calls: u64,
    pub(crate) sun_draw_calls: u64,
    pub(crate) moon_draw_calls: u64,
    pub(crate) shadow_draw_calls: u64,
    pub(crate) frames_skipped: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EnvironmentStats {
    cpu: MicrosecondHistogram,
    upload_cpu: MicrosecondHistogram,
    shadow_cpu: MicrosecondHistogram,
    counters: EnvironmentStatsSnapshot,
    astronomy_latest: [u64; 4],
    astronomy_baseline: [u64; 4],
}

impl EnvironmentStats {
    pub(crate) fn record_cpu(&mut self, duration: Duration) {
        self.cpu.record(duration);
    }

    pub(crate) fn record_shadow_cpu(&mut self, duration: Duration) {
        self.shadow_cpu.record(duration);
    }

    pub(crate) fn record_upload_cpu(&mut self, duration: Duration) {
        self.upload_cpu.record(duration);
    }

    pub(crate) fn set_astronomy_counters(
        &mut self,
        ephemeris_evaluations: u64,
        ephemeris_cache_hits: u64,
        star_epoch_updates: u64,
        star_epoch_cache_hits: u64,
    ) {
        self.astronomy_latest = [
            ephemeris_evaluations,
            ephemeris_cache_hits,
            star_epoch_updates,
            star_epoch_cache_hits,
        ];
        self.counters.ephemeris_evaluations =
            self.astronomy_latest[0].saturating_sub(self.astronomy_baseline[0]);
        self.counters.ephemeris_cache_hits =
            self.astronomy_latest[1].saturating_sub(self.astronomy_baseline[1]);
        self.counters.star_epoch_updates =
            self.astronomy_latest[2].saturating_sub(self.astronomy_baseline[2]);
        self.counters.star_epoch_cache_hits =
            self.astronomy_latest[3].saturating_sub(self.astronomy_baseline[3]);
    }

    pub(crate) fn record_frame(
        &mut self,
        presented: bool,
        sky: u32,
        stars: u32,
        sun: u32,
        moon: u32,
        shadows: u32,
    ) {
        if !presented {
            self.counters.frames_skipped = self.counters.frames_skipped.saturating_add(1);
            return;
        }
        self.counters.sky_draw_calls = self.counters.sky_draw_calls.saturating_add(u64::from(sky));
        self.counters.star_draw_calls = self
            .counters
            .star_draw_calls
            .saturating_add(u64::from(stars));
        self.counters.sun_draw_calls = self.counters.sun_draw_calls.saturating_add(u64::from(sun));
        self.counters.moon_draw_calls = self
            .counters
            .moon_draw_calls
            .saturating_add(u64::from(moon));
        self.counters.shadow_draw_calls = self
            .counters
            .shadow_draw_calls
            .saturating_add(u64::from(shadows));
    }

    pub(crate) fn snapshot(self) -> EnvironmentStatsSnapshot {
        EnvironmentStatsSnapshot {
            cpu: self.cpu.snapshot(),
            upload_cpu: self.upload_cpu.snapshot(),
            shadow_cpu: self.shadow_cpu.snapshot(),
            ..self.counters
        }
    }

    pub(crate) fn reset_epoch(&mut self) {
        let astronomy_baseline = self.astronomy_latest;
        *self = Self::default();
        self.astronomy_latest = astronomy_baseline;
        self.astronomy_baseline = astronomy_baseline;
    }
}
