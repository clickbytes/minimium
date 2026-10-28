use std::time::Duration;

const TICK_BOUNDS_MICROS: &[u64] = &[50, 100, 250, 500, 1_000, 2_500, 5_000, 50_000];
// Dense boundaries preserve useful frame-time resolution around common refresh
// targets (8, 16, 20, and 33 ms).
const FRAME_BOUNDS_MICROS: &[u64] = &[
    4_000, 6_000, 8_000, 10_000, 12_000, 14_000, 16_000, 18_000, 20_000, 22_000, 25_000, 28_000,
    33_000, 40_000, 50_000,
];
const RTT_BOUNDS_MICROS: &[u64] = &[
    50, 100, 250, 500, 1_000, 2_000, 5_000, 10_000, 20_000, 50_000, 100_000, 250_000, 500_000,
    1_000_000, 2_500_000, 5_000_000,
];
const MAX_BUCKETS: usize = 17;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BucketSet {
    Tick,
    Frame,
    Rtt,
}

impl BucketSet {
    fn bounds(self) -> &'static [u64] {
        match self {
            Self::Tick => TICK_BOUNDS_MICROS,
            Self::Frame => FRAME_BOUNDS_MICROS,
            Self::Rtt => RTT_BOUNDS_MICROS,
        }
    }
}

/// Fixed-storage duration histogram with source-specific bucket boundaries.
/// Recording and merging never allocate.
pub struct DurationStats {
    bucket_set: BucketSet,
    samples: u64,
    total_nanoseconds: u128,
    maximum_nanoseconds: u64,
    minimum_nanoseconds: Option<u64>,
    buckets: [u64; MAX_BUCKETS],
}

/// Histogram for simulation ticks and server work phases.
pub type TickDurationStats = DurationStats;
/// Histogram for redraw intervals and client render work.
pub type FrameDurationStats = DurationStats;
/// Histogram for generator-observed ping/pong round-trip time.
pub type RttDurationStats = DurationStats;

impl Default for DurationStats {
    fn default() -> Self {
        Self::with_bucket_set(BucketSet::Tick)
    }
}

impl DurationStats {
    pub fn ticks() -> Self {
        Self::with_bucket_set(BucketSet::Tick)
    }

    pub fn frames() -> Self {
        Self::with_bucket_set(BucketSet::Frame)
    }

    pub fn rtt() -> Self {
        Self::with_bucket_set(BucketSet::Rtt)
    }

    fn with_bucket_set(bucket_set: BucketSet) -> Self {
        Self {
            bucket_set,
            samples: 0,
            total_nanoseconds: 0,
            maximum_nanoseconds: 0,
            minimum_nanoseconds: None,
            buckets: [0; MAX_BUCKETS],
        }
    }

    pub fn record(&mut self, duration: Duration) {
        let nanoseconds = duration.as_nanos().min(u128::from(u64::MAX)) as u64;
        let bounds = self.bucket_set.bounds();
        let bucket = bounds
            .iter()
            .position(|limit| nanoseconds <= limit.saturating_mul(1_000))
            .unwrap_or(bounds.len());
        self.samples = self.samples.saturating_add(1);
        self.total_nanoseconds = self
            .total_nanoseconds
            .saturating_add(u128::from(nanoseconds));
        self.minimum_nanoseconds = Some(
            self.minimum_nanoseconds
                .unwrap_or(nanoseconds)
                .min(nanoseconds),
        );
        self.maximum_nanoseconds = self.maximum_nanoseconds.max(nanoseconds);
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
    }

    pub fn merge(&mut self, other: &Self) {
        assert_eq!(
            self.bucket_set, other.bucket_set,
            "histogram bucket sets differ"
        );
        self.samples = self.samples.saturating_add(other.samples);
        self.total_nanoseconds = self
            .total_nanoseconds
            .saturating_add(other.total_nanoseconds);
        self.minimum_nanoseconds = match (self.minimum_nanoseconds, other.minimum_nanoseconds) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (None, value) | (value, None) => value,
        };
        self.maximum_nanoseconds = self.maximum_nanoseconds.max(other.maximum_nanoseconds);
        for (total, count) in self.buckets.iter_mut().zip(other.buckets) {
            *total = total.saturating_add(count);
        }
    }

    pub fn samples(&self) -> u64 {
        self.samples
    }

    pub fn minimum_microseconds(&self) -> Option<u64> {
        self.minimum_nanoseconds.map(|value| value / 1_000)
    }

    pub fn average_milliseconds(&self) -> Option<f64> {
        (self.samples > 0)
            .then(|| self.total_nanoseconds as f64 / self.samples as f64 / 1_000_000.0)
    }

    pub fn percentile_milliseconds(&self, percentile: u64) -> Option<f64> {
        self.percentile_microseconds(percentile)
            .map(|value| value as f64 / 1_000.0)
    }

    pub fn minimum_milliseconds(&self) -> Option<f64> {
        self.minimum_microseconds()
            .map(|value| value as f64 / 1_000.0)
    }

    pub fn maximum_milliseconds(&self) -> Option<f64> {
        (self.samples > 0).then(|| self.maximum_nanoseconds as f64 / 1_000_000.0)
    }

    pub fn average_microseconds(&self) -> Option<u64> {
        (self.samples > 0).then(|| {
            (self.total_nanoseconds / u128::from(self.samples) / 1_000).min(u128::from(u64::MAX))
                as u64
        })
    }

    pub fn percentile_microseconds(&self, percentile: u64) -> Option<u64> {
        if self.samples == 0 {
            return None;
        }
        let percentile = percentile.clamp(1, 100);
        let rank = self.samples.saturating_mul(percentile).saturating_add(99) / 100;
        let mut cumulative = 0u64;
        for (index, count) in self.buckets.iter().enumerate() {
            cumulative = cumulative.saturating_add(*count);
            if cumulative >= rank {
                return Some(
                    self.bucket_set
                        .bounds()
                        .get(index)
                        .copied()
                        .unwrap_or(self.maximum_nanoseconds / 1_000),
                );
            }
        }
        Some(self.maximum_nanoseconds / 1_000)
    }
}
