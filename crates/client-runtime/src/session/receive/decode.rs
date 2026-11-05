use super::*;

impl ClientSession {
    pub(super) fn decode_available(&mut self, events: &mut Vec<SessionEvent>) {
        loop {
            match decode_server(&self.receive_buffer) {
                Ok(Some((message, frame_bytes))) => {
                    self.receive_buffer.drain(..frame_bytes);
                    let decoded_at_ms = self.clock.local_monotonic_ms();
                    let planet = match &message {
                        ServerMessage::Welcome { manifest, .. }
                        | ServerMessage::TransitionPrepare { manifest, .. }
                        | ServerMessage::TransitionCommitted { manifest, .. } => manifest.planet(),
                        _ => None,
                    };
                    if let Some(definition) = planet {
                        let package = std::env::var_os("MINIMIUM_EARTH_PACKAGE")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_else(|| std::path::PathBuf::from("third_party/gebco-2026"));
                        let already_loaded =
                            self.prepared_geography.as_ref().is_some_and(|geography| {
                                geography.package_digest() == definition.package_digest
                            });
                        if !already_loaded {
                            let prepared = match minimium_asset_io::open_earth_package(
                                &package,
                                Some(definition.package_digest),
                            ) {
                                Ok(prepared) => prepared,
                                Err(error) => {
                                    self.disconnect(DisconnectReason::EarthPackage(error), events);
                                    return;
                                }
                            };
                            self.prepared_geography = Some(prepared);
                        }
                    }
                    match &message {
                        ServerMessage::Welcome {
                            map_fingerprint,
                            context,
                            manifest,
                            clock_sample,
                            environment,
                            ..
                        } => {
                            if self.state != SessionState::AwaitingWelcome {
                                self.disconnect(DisconnectReason::UnexpectedMessage, events);
                                return;
                            }
                            let expected_fingerprint =
                                self.expected_fingerprint(context.instance(), manifest);
                            if *map_fingerprint != expected_fingerprint {
                                self.disconnect(
                                    DisconnectReason::MapMismatch {
                                        expected: expected_fingerprint,
                                        actual: *map_fingerprint,
                                    },
                                    events,
                                );
                                return;
                            }
                            if manifest.content_revision() != context.content_revision() {
                                self.disconnect(DisconnectReason::GeneratorMismatch, events);
                                return;
                            }
                            if manifest.generator_digest()
                                != minimium_world::world::GeneratorDigest::legacy_map_fingerprint(
                                    expected_fingerprint,
                                )
                                && manifest.generator_digest()
                                    != minimium_procgen::generator_digest(
                                        manifest.topology(),
                                        manifest.generation(),
                                    )
                            {
                                self.disconnect(DisconnectReason::GeneratorMismatch, events);
                                return;
                            }
                            if let Err(reason) = self.install_initial_environment(
                                *manifest,
                                *clock_sample,
                                *environment,
                            ) {
                                self.disconnect(reason, events);
                                return;
                            }
                            self.world_context = Some(*context);
                            self.pending_transition = None;
                            self.ready_transition = None;
                            self.state = SessionState::Connected;
                            self.state_since = Instant::now();
                            self.last_heartbeat_at = self.state_since;
                        }
                        ServerMessage::TransitionPrepare {
                            transition_id,
                            source_context,
                            target_context,
                            map_fingerprint,
                            manifest,
                            clock_sample,
                            environment,
                            ..
                        } => {
                            if self.state != SessionState::Connected {
                                self.disconnect(DisconnectReason::UnexpectedMessage, events);
                                return;
                            }
                            if Some(*source_context) != self.world_context {
                                continue;
                            }
                            let expected_fingerprint =
                                self.expected_fingerprint(target_context.instance(), manifest);
                            if *map_fingerprint != expected_fingerprint {
                                self.disconnect(
                                    DisconnectReason::MapMismatch {
                                        expected: expected_fingerprint,
                                        actual: *map_fingerprint,
                                    },
                                    events,
                                );
                                return;
                            }
                            if manifest.content_revision() != target_context.content_revision()
                                || !manifest_matches_generator(manifest)
                            {
                                self.disconnect(DisconnectReason::GeneratorMismatch, events);
                                return;
                            }
                            let transition = (*transition_id, *target_context);
                            if self
                                .pending_transition
                                .is_some_and(|current| current != transition)
                            {
                                self.disconnect(DisconnectReason::UnexpectedMessage, events);
                                return;
                            }
                            if let Err(reason) = self.prepare_environment(
                                *transition_id,
                                *target_context,
                                *manifest,
                                *clock_sample,
                                *environment,
                            ) {
                                self.disconnect(reason, events);
                                return;
                            }
                            self.pending_transition = Some(transition);
                            self.ready_transition = None;
                        }
                        ServerMessage::TransitionCommitted {
                            transition_id,
                            context,
                            map_fingerprint,
                            manifest,
                            clock_sample,
                            environment,
                            ..
                        } => {
                            let expected_fingerprint =
                                self.expected_fingerprint(context.instance(), manifest);
                            if *map_fingerprint != expected_fingerprint {
                                self.disconnect(
                                    DisconnectReason::MapMismatch {
                                        expected: expected_fingerprint,
                                        actual: *map_fingerprint,
                                    },
                                    events,
                                );
                                return;
                            }
                            if manifest.content_revision() != context.content_revision()
                                || !manifest_matches_generator(manifest)
                            {
                                self.disconnect(DisconnectReason::GeneratorMismatch, events);
                                return;
                            }
                            if self.pending_transition != Some((*transition_id, *context)) {
                                continue;
                            }
                            if let Err(reason) = self.commit_environment(
                                *transition_id,
                                *context,
                                *manifest,
                                *clock_sample,
                                *environment,
                            ) {
                                self.disconnect(reason, events);
                                return;
                            }
                            self.world_context = Some(*context);
                            self.pending_transition = None;
                            self.ready_transition = None;
                            // Keep queued bytes: a partially sent frame must be completed.
                            // The server rejects world inputs carrying the old context;
                            // heartbeat and clock probes remain valid across the transfer.
                        }
                        ServerMessage::TransitionCancelled {
                            transition_id,
                            context,
                        } => {
                            if !self
                                .pending_transition
                                .is_some_and(|(pending, _)| pending == *transition_id)
                                || Some(*context) != self.world_context
                            {
                                continue;
                            }
                            self.pending_transition = None;
                            self.ready_transition = None;
                            self.cancel_pending_environment(*transition_id);
                        }
                        ServerMessage::EnvironmentSync {
                            context,
                            clock_sample,
                            environment,
                        } => {
                            if self.state != SessionState::Connected {
                                self.disconnect(DisconnectReason::UnexpectedMessage, events);
                                return;
                            }
                            match self.apply_environment_sync(*context, *clock_sample, *environment)
                            {
                                Ok(true) => {}
                                Ok(false) => continue,
                                Err(reason) => {
                                    self.disconnect(reason, events);
                                    return;
                                }
                            }
                        }
                        ServerMessage::TimeProbeReply {
                            seq,
                            echoed_client_send_monotonic_ms,
                            server_receive_monotonic_ms,
                            server_send_monotonic_ms,
                            session_nonce,
                        } => {
                            if let Err(reason) = self.validate_probe_nonce(*session_nonce) {
                                self.disconnect(reason, events);
                                return;
                            }
                            self.clock.accept_probe_reply(
                                *seq,
                                *echoed_client_send_monotonic_ms,
                                *server_receive_monotonic_ms,
                                *server_send_monotonic_ms,
                                decoded_at_ms,
                            );
                        }
                        ServerMessage::Error(code)
                            if self.state == SessionState::AwaitingWelcome =>
                        {
                            let code = *code;
                            self.disconnect(DisconnectReason::Rejected(code), events);
                            return;
                        }
                        ServerMessage::Bye if self.state == SessionState::Connected => {
                            self.disconnect(DisconnectReason::ServerClosed, events);
                            return;
                        }
                        _ if self.state != SessionState::Connected => {
                            self.disconnect(DisconnectReason::UnexpectedMessage, events);
                            return;
                        }
                        _ => {}
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
            }
        }
    }
}
