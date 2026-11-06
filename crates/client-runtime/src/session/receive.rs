mod decode;

use super::*;

impl ClientSession {
    fn expected_fingerprint(
        &self,
        instance: minimium_world::world::WorldInstanceId,
        manifest: &minimium_world::world::WorldManifest,
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
                minimium_game_sample::generate_worlds_with_planet_revision_and_geography(
                    seed,
                    planet,
                    manifest.generation().revision().get(),
                    self.prepared_geography(),
                )
            } else {
                minimium_game_sample::generate_worlds(seed)
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
        if let Err(error) = self.poll_content() {
            self.disconnect(
                DisconnectReason::Materials(format!("{error}; reconnect to retry")),
                events,
            );
            return self.state;
        }
        let now = Instant::now();
        self.clock.observe_poll(self.clock.local_monotonic_ms());
        if self.state == SessionState::AwaitingWelcome
            && self.content.started.is_none()
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
        self.close();
        events.push(SessionEvent::Disconnected(reason));
    }
}
