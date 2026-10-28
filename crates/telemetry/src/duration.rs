use std::time::Duration;
const DURATION_BUCKETS_MICROS: [u64; 8] = [50, 100, 250, 500, 1_000, 2_500, 5_000, 50_000];

#[derive(Default)]
pub struct DurationStats {
    samples: u64,
    total_nanoseconds: u128,
    maximum_nanoseconds: u64,
    buckets: [u64; DURATION_BUCKETS_MICROS.len() + 1],
}

impl DurationStats {
    pub fn record(&mut self, duration: Duration) {
        let nanoseconds = duration.as_nanos().min(u128::from(u64::MAX)) as u64;
        let microseconds = duration.as_micros().min(u128::from(u64::MAX)) as u64;
        let bucket = DURATION_BUCKETS_MICROS
            .iter()
            .position(|limit| microseconds <= *limit)
            .unwrap_or(DURATION_BUCKETS_MICROS.len());
        self.samples = self.samples.saturating_add(1);
        self.total_nanoseconds = self
            .total_nanoseconds
            .saturating_add(u128::from(nanoseconds));
        self.maximum_nanoseconds = self.maximum_nanoseconds.max(nanoseconds);
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
    }

    pub fn average_microseconds(&self) -> u64 {
        if self.samples == 0 {
            return 0;
        }
        (self.total_nanoseconds / u128::from(self.samples) / 1_000).min(u128::from(u64::MAX)) as u64
    }

    pub fn percentile_microseconds(&self, percentile: u64) -> u64 {
        if self.samples == 0 {
            return 0;
        }
        let rank = self.samples.saturating_mul(percentile).saturating_add(99) / 100;
        let mut cumulative = 0u64;
        for (index, count) in self.buckets.iter().enumerate() {
            cumulative = cumulative.saturating_add(*count);
            if cumulative >= rank {
                return DURATION_BUCKETS_MICROS
                    .get(index)
                    .copied()
                    .unwrap_or(self.maximum_nanoseconds / 1_000);
            }
        }
        self.maximum_nanoseconds / 1_000
    }
}
