use super::*;

impl ClientSession {
    fn expected_fingerprint(
        &self,
        instance: minimium_core::world::WorldInstanceId,
        manifest: &minimium_core::world::WorldManifest,
    ) -> u64 {
        if matches!(manifest.generation().revision().get(), 2..=4)
            && (instance.get() == 1 || instance.get() == 2)
        {
            let seed = manifest.generation().seed().get()
                ^ if instance.get() == 2 {
                    0x0030_0000_0000_0003
                } else {
                    0
                };
            let generated = if let Some(planet) = manifest.planet() {
                minimium_procgen::generate_worlds_with_planet_revision(
                    seed,
                    planet,
                    manifest.generation().revision().get(),
                )
            } else {
                minimium_procgen::generate_worlds(seed)
            };
            if let Ok(worlds) = generated {
                let world = &worlds[instance.get() as usize - 1];
                if world.manifest == *manifest {
                    return world.map.fingerprint();
                }
            }
            return 0;
        }
        self.world_fingerprints
            .get(&instance)
            .copied()
            .unwrap_or(self.expected_map_fingerprint)
    }
    /// Performs bounded nonblocking reads, flushes queued writes (including
    /// partial writes), validates protocol state, and maintains heartbeat
    /// timers. Appends events to caller-owned storage and returns the current
    /// state. Fatal conditions append one Disconnected event and clear all
    /// buffers and connection state.
    pub fn poll(&mut self, events: &mut Vec<SessionEvent>) -> SessionState {
        let now = Instant::now();
        self.clock.observe_poll(self.clock.local_monotonic_ms());
        if self.state == SessionState::AwaitingWelcome
            && now.saturating_duration_since(self.state_since) >= WELCOME_TIMEOUT
        {
            self.disconnect(DisconnectReason::WelcomeTimeout, events);
            return self.state;
        }
        if self.state == SessionState::Connected
            && self
                .pending_ping_at
                .is_some_and(|sent| now.saturating_duration_since(sent) >= HEARTBEAT_TIMEOUT)
        {
            self.disconnect(DisconnectReason::HeartbeatTimeout, events);
            return self.state;
        }

        self.flush_send(events);
        if self.state == SessionState::Disconnected {
            return self.state;
        }

        self.read_available(events);
        if self.state == SessionState::Disconnected {
            return self.state;
        }
        self.decode_available(events);
        if self.state == SessionState::Disconnected {
            return self.state;
        }

        let now = Instant::now();
        let now_ms = self.clock.local_monotonic_ms();
        self.queue_due_time_probe(now_ms);
        if self.state == SessionState::Connected
            && self.pending_ping_at.is_none()
            && self.pending_ping_end.is_none()
            && now.saturating_duration_since(self.last_heartbeat_at) >= HEARTBEAT_INTERVAL
            && self.queue(&ClientMessage::Ping).is_ok()
        {
            self.last_heartbeat_at = now;
        }
        self.flush_send(events);
        let now_ms = self.clock.local_monotonic_ms();
        if self.state == SessionState::Connected {
            self.clock.update_frame(
                self.clock_sample,
                self.environment_descriptor,
                self.world_context,
                now_ms,
            );
        } else {
            self.clock.update_frame(None, None, None, now_ms);
        }
        self.state
    }

    pub(super) fn append_encoded(&mut self, message: &ClientMessage) -> Result<(), ProtocolError> {
        let frame = encode_client(message)?;
        self.append_frame(&frame)
            .map_err(|()| ProtocolError::PayloadTooLarge)
    }

    pub(super) fn append_frame(&mut self, frame: &[u8]) -> Result<(), ()> {
        if self.send_offset > 0 {
            if let Some(end) = &mut self.pending_ping_end {
                *end = end.saturating_sub(self.send_offset);
            }
            self.send_buffer.drain(..self.send_offset);
            self.send_offset = 0;
        }
        if self.send_buffer.len().saturating_add(frame.len()) > MAX_SESSION_BUFFER_BYTES {
            return Err(());
        }
        self.send_buffer.extend_from_slice(frame);
        Ok(())
    }

    fn flush_send(&mut self, events: &mut Vec<SessionEvent>) {
        while self.send_offset < self.send_buffer.len() {
            let result = match self.stream.as_mut() {
                Some(stream) => stream.write(&self.send_buffer[self.send_offset..]),
                None => return,
            };
            match result {
                Ok(0) => {
                    self.disconnect(DisconnectReason::Io(io::ErrorKind::WriteZero), events);
                    return;
                }
                Ok(count) => {
                    self.send_offset += count;
                    self.metrics.bytes_sent = self.metrics.bytes_sent.saturating_add(count as u64);
                    if self
                        .pending_ping_end
                        .is_some_and(|end| self.send_offset >= end)
                    {
                        self.pending_ping_end = None;
                        self.pending_ping_at = Some(Instant::now());
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    self.disconnect(DisconnectReason::Io(error.kind()), events);
                    return;
                }
            }
        }
        if self.send_offset == self.send_buffer.len() {
            self.send_buffer.clear();
            self.send_offset = 0;
        }
    }

    fn read_available(&mut self, events: &mut Vec<SessionEvent>) {
        let mut chunk = [0u8; 4096];
        let mut total_read = 0;
        loop {
            let available = (MAX_SESSION_READ_BYTES_PER_POLL - total_read).min(chunk.len());
            if available == 0 {
                break;
            }
            let result = match self.stream.as_mut() {
                Some(stream) => stream.read(&mut chunk[..available]),
                None => return,
            };
            match result {
                Ok(0) => {
                    self.decode_available(events);
                    if self.state != SessionState::Disconnected {
                        self.disconnect(DisconnectReason::PeerClosed, events);
                    }
                    return;
                }
                Ok(count) => {
                    total_read += count;
                    self.metrics.bytes_received =
                        self.metrics.bytes_received.saturating_add(count as u64);
                    if self.receive_buffer.len().saturating_add(count) > MAX_SESSION_BUFFER_BYTES {
                        self.disconnect(DisconnectReason::BufferLimit, events);
                        return;
                    }
                    self.receive_buffer.extend_from_slice(&chunk[..count]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    self.disconnect(DisconnectReason::Io(error.kind()), events);
                    return;
                }
            }
        }
    }

    fn decode_available(&mut self, events: &mut Vec<SessionEvent>) {
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
                        if let Err(error) = minimium_procgen::ensure_earth_from_manifest(definition)
                        {
                            self.disconnect(DisconnectReason::EarthPackage(error), events);
                            return;
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
                                != minimium_core::world::GeneratorDigest::legacy_map_fingerprint(
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

    fn disconnect(&mut self, reason: DisconnectReason, events: &mut Vec<SessionEvent>) {
        self.close();
        events.push(SessionEvent::Disconnected(reason));
    }
}
