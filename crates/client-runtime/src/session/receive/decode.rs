mod dispatch;

use super::*;

impl ClientSession {
    pub(in crate::session) fn decode_available(&mut self, events: &mut Vec<SessionEvent>) {
        loop {
            if let Some((deferred, decoded_at_ms)) = self.deferred_world_message.take() {
                if self.world_content_pending() {
                    if self.world_content_started.is_some_and(|started| {
                        self.now.elapsed_since(started) >= Duration::from_secs(300)
                    }) {
                        self.cancel_world_content_job();
                        let mut error = SessionFailure::new(
                            "session.world_content_timeout",
                            "world-content preparation exceeded 300 seconds",
                        );
                        if let Some(ticket) = self
                            .world_content_job
                            .as_ref()
                            .and_then(|handle| match handle.ticket() {
                                crate::ContentJobTicket::World(ticket) => Some(ticket),
                                _ => None,
                            })
                            .or(self.world_content_request.map(|request| request.ticket))
                        {
                            error = error.with_content_ticket(ticket);
                        }
                        self.log_failure(&error);
                        self.disconnect(DisconnectReason::WorldContent(error), events);
                        return;
                    }
                    self.deferred_world_message = Some((deferred.clone(), decoded_at_ms));
                    let next = match self.front_message() {
                        Ok(Some(message)) => message,
                        Ok(None) => return,
                        Err(reason) => {
                            self.disconnect(reason, events);
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
                            self.consume_message();
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
                            self.cancel_world_content_job();
                            self.mark_transition_terminated(*pending_id);
                            self.pending_world_content = None;
                            self.deferred_world_message = None;
                            self.world_content_request = None;
                            self.world_content_started = None;
                            self.consume_message();
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
                            self.cancel_world_content_job();
                            self.pending_world_content = None;
                            self.world_content_request = None;
                            self.world_content_started = Some(self.now);
                            self.deferred_world_message =
                                Some((next.clone(), self.clock.local_monotonic_ms()));
                            self.consume_message();
                            continue;
                        }
                        (_, ServerMessage::Error(_) | ServerMessage::Bye) => {
                            self.cancel_world_content_job();
                            self.deferred_world_message = None;
                            self.world_content_request = None;
                            self.world_content_started = None;
                            self.consume_message();
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
            let message = match self.front_message() {
                Ok(Some(message)) => message,
                Ok(None) => return,
                Err(reason) => {
                    self.disconnect(reason, events);
                    return;
                }
            };

            let decoded_at_ms = self.clock.local_monotonic_ms();
            match dispatch::dispatch_message(self, &message, decoded_at_ms, events) {
                dispatch::DispatchResult::Stop => return,
                dispatch::DispatchResult::Pending => {
                    self.deferred_world_message = Some((message, decoded_at_ms));
                    self.consume_message();
                    continue;
                }
                dispatch::DispatchResult::Skip => {
                    self.consume_message();
                    continue;
                }
                dispatch::DispatchResult::Emit => {}
            }
            self.consume_message();

            if let ServerMessage::Snapshot { context, .. } = &message {
                if Some(*context) != self.world_context {
                    continue;
                }
            }
            match &message {
                ServerMessage::Welcome { server_tick, .. }
                | ServerMessage::Snapshot { server_tick, .. } => {
                    self.server_tick = *server_tick;
                }
                _ => {}
            }
            if matches!(&message, ServerMessage::Pong) {
                if let Some(sent_at) = self.pending_ping_at.take() {
                    let rtt = self.now.elapsed_since(sent_at);
                    self.metrics.ping_sample_at = Some(self.now);
                    self.metrics.ping_round_trip = Some(rtt);
                    events.push(SessionEvent::PingRoundTrip(rtt));
                    continue;
                }
            }
            events.push(SessionEvent::Message(message));
        }
    }
}
