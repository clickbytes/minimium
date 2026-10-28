use minimium_telemetry::duration::RttDurationStats;
use std::time::Duration;

#[derive(Default)]
pub(super) struct WorkerStats {
    pub(super) connected: u64,
    pub(super) ready: u64,
    pub(super) handshake_failures: u64,
    pub(super) disconnects_before_start: u64,
    pub(super) disconnects_during_measurement: u64,
    pub(super) connected_at_end: u64,
    pub(super) pongs: u64,
    pub(super) errors: u64,
    pub(super) timeouts: u64,
    pub(super) moves_sent: u64,
    pub(super) shots_sent: u64,
    pub(super) shots_hit: u64,
    pub(super) kills: u64,
    pub(super) chat_sent: u64,
    pub(super) chat_received: u64,
    pub(super) rate_limited: u64,
    pub(super) snapshots_received: u64,
    pub(super) bytes_sent: u64,
    pub(super) bytes_received: u64,
    pub(super) setup_bytes_sent: u64,
    pub(super) setup_bytes_received: u64,
    pub(super) per_client: Vec<ClientMeasurement>,
    pub(super) rtt: RttStats,
}

#[derive(Clone)]
pub(super) struct ClientMeasurement {
    pub(super) generator_id: usize,
    pub(super) bytes_sent: u64,
    pub(super) bytes_received: u64,
    pub(super) disconnected: bool,
}

pub(super) struct RttStats {
    pub(super) samples: u64,
    pub(super) histogram: RttDurationStats,
}

impl Default for RttStats {
    fn default() -> Self {
        Self {
            samples: 0,
            histogram: RttDurationStats::rtt(),
        }
    }
}

impl RttStats {
    pub(super) fn record(&mut self, duration: Duration) {
        self.histogram.record(duration);
        self.samples = self.samples.saturating_add(1);
    }

    pub(super) fn merge(&mut self, other: Self) {
        self.histogram.merge(&other.histogram);
        self.samples = self.samples.saturating_add(other.samples);
    }

    pub(super) fn average_milliseconds(&self) -> Option<f64> {
        self.histogram.average_milliseconds()
    }

    pub(super) fn percentile_milliseconds(&self, percentile: u64) -> Option<f64> {
        self.histogram.percentile_milliseconds(percentile)
    }

    pub(super) fn minimum_milliseconds(&self) -> Option<f64> {
        self.histogram.minimum_milliseconds()
    }

    pub(super) fn maximum_milliseconds(&self) -> Option<f64> {
        self.histogram.maximum_milliseconds()
    }
}

impl WorkerStats {
    pub(super) fn merge(&mut self, other: Self) {
        self.connected = self.connected.saturating_add(other.connected);
        self.ready = self.ready.saturating_add(other.ready);
        self.handshake_failures = self
            .handshake_failures
            .saturating_add(other.handshake_failures);
        self.disconnects_before_start = self
            .disconnects_before_start
            .saturating_add(other.disconnects_before_start);
        self.disconnects_during_measurement = self
            .disconnects_during_measurement
            .saturating_add(other.disconnects_during_measurement);
        self.connected_at_end = self.connected_at_end.saturating_add(other.connected_at_end);
        self.pongs = self.pongs.saturating_add(other.pongs);
        self.errors = self.errors.saturating_add(other.errors);
        self.timeouts = self.timeouts.saturating_add(other.timeouts);
        self.moves_sent = self.moves_sent.saturating_add(other.moves_sent);
        self.shots_sent = self.shots_sent.saturating_add(other.shots_sent);
        self.shots_hit = self.shots_hit.saturating_add(other.shots_hit);
        self.kills = self.kills.saturating_add(other.kills);
        self.chat_sent = self.chat_sent.saturating_add(other.chat_sent);
        self.chat_received = self.chat_received.saturating_add(other.chat_received);
        self.rate_limited = self.rate_limited.saturating_add(other.rate_limited);
        self.snapshots_received = self
            .snapshots_received
            .saturating_add(other.snapshots_received);
        self.bytes_sent = self.bytes_sent.saturating_add(other.bytes_sent);
        self.bytes_received = self.bytes_received.saturating_add(other.bytes_received);
        self.setup_bytes_sent = self.setup_bytes_sent.saturating_add(other.setup_bytes_sent);
        self.setup_bytes_received = self
            .setup_bytes_received
            .saturating_add(other.setup_bytes_received);
        self.per_client.extend(other.per_client);
        self.rtt.merge(other.rtt);
    }
}
