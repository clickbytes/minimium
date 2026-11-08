mod decode;

use super::*;

pub(super) enum WorldContentProgress {
    Ready,
    Pending,
}

impl ClientSession {
    pub(super) fn prepare_world_content(
        &mut self,
        instance: minimium_world::world::WorldInstanceId,
        manifest: minimium_world::world::WorldManifest,
        expected_fingerprint: u64,
    ) -> Result<WorldContentProgress, SessionFailure> {
        let ticket = match self.world_content_request.filter(|ticket| {
            ticket.instance == instance
                && ticket.content_revision == manifest.content_revision().get()
        }) {
            Some(ticket) => ticket,
            None => {
                let request_id = self.next_world_content_request_id;
                self.next_world_content_request_id =
                    request_id.checked_add(1).ok_or_else(|| {
                        SessionFailure::new(
                            "session.world_content_ticket_exhausted",
                            "world-content request identifiers are exhausted",
                        )
                    })?;
                let ticket = ContentResolutionTicket {
                    session_id: self.session_id,
                    request_id,
                    instance,
                    content_revision: manifest.content_revision().get(),
                };
                self.world_content_request = Some(ticket);
                ticket
            }
        };
        if let Some(content) = self.resolved_worlds.get(&instance).filter(|content| {
            content.ticket.session_id == ticket.session_id
                && content.ticket.instance == ticket.instance
                && content.ticket.content_revision == ticket.content_revision
                && content.manifest == manifest
                && content.map.fingerprint() == expected_fingerprint
        }) {
            self.prepared_geography = content.geography.clone();
            self.world_content_request = None;
            return Ok(WorldContentProgress::Ready);
        }
        if self
            .world_content_started
            .is_some_and(|started| started.elapsed() >= Duration::from_secs(300))
        {
            self.world_content_cancel.cancel();
            return Err(SessionFailure::new(
                "session.world_content_timeout",
                "world-content preparation exceeded 300 seconds",
            ));
        }
        if let Some(worker) = self.world_content_worker.as_ref() {
            if self.world_content_worker_ticket == Some(ticket) && !worker.is_finished() {
                return Ok(WorldContentProgress::Pending);
            }
            if self.world_content_worker_ticket != Some(ticket) {
                self.world_content_cancel.cancel();
                if !worker.is_finished() {
                    return Ok(WorldContentProgress::Pending);
                }
            }
        }
        if let Some(worker) = self.world_content_worker.take() {
            let completed_ticket = self.world_content_worker_ticket.take();
            let result = worker.join().map_err(|_| {
                SessionFailure::new(
                    "session.world_content_worker_stopped",
                    "world-content worker stopped unexpectedly",
                )
            })?;
            if completed_ticket != Some(ticket) {
                return self.start_world_content_resolution(ticket, manifest, expected_fingerprint);
            }
            let content = result?;
            self.install_resolved_world_content(content, ticket, manifest, expected_fingerprint)?;
            return Ok(WorldContentProgress::Ready);
        }
        self.start_world_content_resolution(ticket, manifest, expected_fingerprint)
    }

    fn start_world_content_resolution(
        &mut self,
        ticket: ContentResolutionTicket,
        manifest: minimium_world::world::WorldManifest,
        expected_fingerprint: u64,
    ) -> Result<WorldContentProgress, SessionFailure> {
        let resolver = self.world_content_resolver.clone().ok_or_else(|| {
            SessionFailure::new(
                "session.world_resolver_missing",
                "no application world-content resolver is installed",
            )
        })?;
        let request = WorldContentRequest {
            ticket,
            manifest,
            expected_fingerprint,
        };
        self.world_content_cancel = crate::ContentCancellation::default();
        let cancellation = self.world_content_cancel.clone();
        self.world_content_worker = Some(
            std::thread::Builder::new()
                .name("world-content-prepare".into())
                .spawn(move || resolver.resolve(request, cancellation))
                .map_err(|error| {
                    SessionFailure::caused_by(
                        "session.world_content_worker_start_failed",
                        "could not start world-content worker",
                        error,
                    )
                })?,
        );
        self.world_content_worker_ticket = Some(ticket);
        self.world_content_started = Some(Instant::now());
        Ok(WorldContentProgress::Pending)
    }

    fn install_resolved_world_content(
        &mut self,
        content: ResolvedWorldContent,
        ticket: ContentResolutionTicket,
        manifest: minimium_world::world::WorldManifest,
        expected_fingerprint: u64,
    ) -> Result<(), SessionFailure> {
        if !content.matches(ticket, manifest) {
            return Err(SessionFailure::new(
                "session.world_resolver_manifest_mismatch",
                "world-content resolver returned a stale or different manifest",
            ));
        }
        if content.map.fingerprint() != expected_fingerprint {
            return Err(SessionFailure::new(
                "session.world_fingerprint_mismatch",
                "resolved world map does not match the server fingerprint",
            ));
        }
        if let Some(planet) = manifest.planet().filter(|planet| planet.mode == 1) {
            let geography = content.geography.as_ref().ok_or_else(|| {
                SessionFailure::new(
                    "session.earth_geography_missing",
                    "Earth world resolver returned no prepared geography",
                )
            })?;
            if geography.package_digest() != planet.package_digest {
                return Err(SessionFailure::new(
                    "session.earth_geography_digest_mismatch",
                    "prepared Earth geography does not match the world manifest",
                ));
            }
        }
        self.prepared_geography = content.geography.clone();
        let active_instance = self.world_context.map(WorldContext::instance);
        self.resolved_worlds.retain(|known, _| {
            *known == ticket.instance || active_instance.is_some_and(|active| active == *known)
        });
        self.resolved_worlds
            .insert(ticket.instance, Arc::new(content));
        self.world_content_started = None;
        self.world_content_request = None;
        Ok(())
    }
    /// Performs bounded nonblocking reads, flushes queued writes (including
    /// partial writes), validates protocol state, and maintains heartbeat
    /// timers. Appends events to caller-owned storage and returns the current
    /// state. Fatal conditions append one Disconnected event and clear all
    /// buffers and connection state.
    pub fn poll(&mut self, events: &mut Vec<SessionEvent>) -> SessionState {
        if self.deferred_world_message.is_none()
            && self
                .world_content_worker
                .as_ref()
                .is_some_and(|worker| worker.is_finished())
        {
            if let Some(worker) = self.world_content_worker.take() {
                let _ = worker.join();
            }
            self.world_content_worker_ticket = None;
            self.world_content_started = None;
        }
        if let Err(error) = self.poll_content() {
            self.log_failure(&error);
            self.disconnect(
                DisconnectReason::Materials(SessionFailure::new(
                    error.code(),
                    format!("{error}; reconnect to retry"),
                )),
                events,
            );
            return self.state;
        }
        let now = Instant::now();
        self.clock.observe_poll(self.clock.local_monotonic_ms());
        if self.state == SessionState::AwaitingWelcome
            && self.content.started.is_none()
            && self.world_content_started.is_none()
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

    pub(super) fn disconnect(&mut self, reason: DisconnectReason, events: &mut Vec<SessionEvent>) {
        self.clear_replicas();
        self.close();
        events.push(SessionEvent::Disconnected(reason));
    }
}
