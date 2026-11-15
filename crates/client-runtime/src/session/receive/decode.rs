mod dispatch;

use super::*;

impl ClientSession {
    pub(super) fn decode_available(&mut self, events: &mut Vec<SessionEvent>) {
        loop {
            if let Some((deferred, decoded_at_ms)) = self.deferred_world_message.take() {
                if self
                    .world_content_worker
                    .as_ref()
                    .is_some_and(|worker| !worker.is_finished())
                {
                    if self
                        .world_content_started
                        .is_some_and(|started| started.elapsed() >= Duration::from_secs(300))
                    {
                        self.world_content_cancel.cancel();
                        let error = SessionFailure::new(
                            "session.world_content_timeout",
                            "world-content preparation exceeded 300 seconds",
                        );
                        self.log_failure(&error);
                        self.disconnect(DisconnectReason::WorldContent(error), events);
                        return;
                    }
                    self.deferred_world_message = Some((deferred.clone(), decoded_at_ms));
                    let (next, frame_bytes) = match decode_server(&self.receive_buffer) {
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
                    match (&deferred, &next) {
                        (
                            previous @ ServerMessage::TransitionPrepare {
                                transition_id: old_id,
                                ..
                            },
                            incoming @ ServerMessage::TransitionPrepare {
                                transition_id: new_id,
                                ..
                            },
                        ) if old_id == new_id => {
                            if previous != incoming {
                                self.disconnect(DisconnectReason::UnexpectedMessage, events);
                                return;
                            }
                            self.receive_buffer.drain(..frame_bytes);
                            continue;
                        }
                        (
                            ServerMessage::TransitionPrepare {
                                transition_id: pending_id,
                                source_context,
                                target_context,
                                ..
                            },
                            ServerMessage::TransitionCancelled {
                                transition_id,
                                context,
                            },
                        ) if pending_id == transition_id
                            && source_context == context
                            && Some(*context) == self.world_context =>
                        {
                            self.world_content_cancel.cancel();
                            self.mark_transition_terminated(*pending_id);
                            self.pending_world_content = None;
                            self.deferred_world_message = None;
                            self.world_content_request = None;
                            self.world_content_started = None;
                            self.receive_buffer.drain(..frame_bytes);
                            continue;
                        }
                        (_, next @ ServerMessage::TransitionPrepare { source_context, .. })
                            if self.state == SessionState::Connected
                                && Some(*source_context) == self.world_context =>
                        {
                            if !self.validate_prepare_message(next) {
                                self.disconnect(DisconnectReason::UnexpectedMessage, events);
                                return;
                            }
                            let incoming_id = match next {
                                ServerMessage::TransitionPrepare { transition_id, .. } => {
                                    *transition_id
                                }
                                _ => unreachable!(),
                            };
                            if let ServerMessage::TransitionPrepare {
                                transition_id: deferred_id,
                                ..
                            } = &deferred
                            {
                                if incoming_id <= *deferred_id {
                                    self.disconnect(DisconnectReason::UnexpectedMessage, events);
                                    return;
                                }
                                self.mark_transition_terminated(*deferred_id);
                            }
                            self.world_content_cancel.cancel();
                            self.pending_world_content = None;
                            self.world_content_request = None;
                            self.world_content_started = Some(Instant::now());
                            self.deferred_world_message =
                                Some((next.clone(), self.clock.local_monotonic_ms()));
                            self.receive_buffer.drain(..frame_bytes);
                            continue;
                        }
                        (_, ServerMessage::Error(_) | ServerMessage::Bye) => {
                            self.world_content_cancel.cancel();
                            self.deferred_world_message = None;
                            self.world_content_request = None;
                            self.world_content_started = None;
                            self.receive_buffer.drain(..frame_bytes);
                            if matches!(
                                dispatch::dispatch_message(
                                    self,
                                    &next,
                                    self.clock.local_monotonic_ms(),
                                    events
                                ),
                                dispatch::DispatchResult::Stop
                            ) {
                                return;
                            }
                            continue;
                        }
                        _ => return,
                    }
                }
                match dispatch::dispatch_message(self, &deferred, decoded_at_ms, events) {
                    dispatch::DispatchResult::Stop => return,
                    dispatch::DispatchResult::Pending => {
                        self.deferred_world_message = Some((deferred, decoded_at_ms));
                        continue;
                    }
                    dispatch::DispatchResult::Skip => continue,
                    dispatch::DispatchResult::Emit => {
                        events.push(SessionEvent::Message(deferred));
                        continue;
                    }
                }
            }
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

            let decoded_at_ms = self.clock.local_monotonic_ms();
            match dispatch::dispatch_message(self, &message, decoded_at_ms, events) {
                dispatch::DispatchResult::Stop => return,
                dispatch::DispatchResult::Pending => {
                    self.deferred_world_message = Some((message, decoded_at_ms));
                    self.receive_buffer.drain(..frame_bytes);
                    continue;
                }
                dispatch::DispatchResult::Skip => {
                    self.receive_buffer.drain(..frame_bytes);
                    continue;
                }
                dispatch::DispatchResult::Emit => {}
            }
            self.receive_buffer.drain(..frame_bytes);

            if let ServerMessage::Snapshot { context, .. } = &message {
                if Some(*context) != self.world_context {
                    continue;
                }
            }
            match &message {
                ServerMessage::Welcome { server_tick, .. }
                | ServerMessage::Snapshot { server_tick, .. } => {
                    self.server_tick = *server_tick;
                    self.next_target_tick = self
                        .next_target_tick
                        .max(server_tick.saturating_add(2).max(1));
                }
                _ => {}
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
