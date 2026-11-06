mod dispatch;

use super::*;

impl ClientSession {
    pub(super) fn decode_available(&mut self, events: &mut Vec<SessionEvent>) {
        loop {
            let (message, frame_bytes) = match decode_server(&self.receive_buffer) {
                Ok(Some(decoded)) => decoded,
                Ok(None) => {
                    if self.receive_buffer.len() > MAX_FRAME_BYTES {
                        self.disconnect(DisconnectReason::BufferLimit, events);
                    }
                    return;
                }
                Err(ProtocolError::UnsupportedEnvironment) => {
                    self.disconnect(DisconnectReason::EnvironmentMismatch, events);
                    return;
                }
                Err(error) => {
                    self.disconnect(DisconnectReason::Protocol(error), events);
                    return;
                }
            };

            self.receive_buffer.drain(..frame_bytes);
            let decoded_at_ms = self.clock.local_monotonic_ms();
            match dispatch::dispatch_message(self, &message, decoded_at_ms, events) {
                dispatch::DispatchResult::Stop => return,
                dispatch::DispatchResult::Skip => continue,
                dispatch::DispatchResult::Emit => {}
            }

            if let ServerMessage::Snapshot { context, .. } = &message {
                if Some(*context) != self.world_context {
                    continue;
                }
            }
            if matches!(&message, ServerMessage::Pong) {
                if let Some(sent_at) = self.pending_ping_at.take() {
                    let rtt = Instant::now().saturating_duration_since(sent_at);
                    self.metrics.ping_round_trip = Some(rtt);
                    events.push(SessionEvent::PingRoundTrip(rtt));
                    continue;
                }
            }
            events.push(SessionEvent::Message(message));
        }
    }
}
