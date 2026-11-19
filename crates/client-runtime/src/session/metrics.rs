use super::*;

/// Scoped to the session that allocated the request and frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueuedWorldFrame {
    pub sequence: u64,
    pub(super) frame: ClientFrameToken,
}

/// Cumulative wire traffic metrics and the most recent measured ping RTT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionMetrics {
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub last_queued_world_frame: Option<QueuedWorldFrame>,
    pub ping_sample_at: Option<ClientTime>,
    pub ping_round_trip: Option<Duration>,
}

impl ClientSession {
    pub fn world_frame_sent(&self, frame: QueuedWorldFrame) -> Option<bool> {
        self.transport.as_ref()?.frame_sent(frame.frame)
    }

    pub fn ping_pending(&self) -> bool {
        self.pending_ping_at.is_some() || self.pending_ping_frame.is_some()
    }

    pub fn metrics(&self) -> SessionMetrics {
        self.metrics
    }
}
