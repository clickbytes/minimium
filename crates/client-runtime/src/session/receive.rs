mod decode;

use super::*;

pub(super) enum WorldContentProgress {
    Ready,
    Pending,
}

impl ClientSession {
    pub(in crate::session) fn install_resolved_world_content(
        &mut self,
        content: ResolvedWorldContent,
        water_catalog: Option<Arc<minimium_procgen::WaterCatalog>>,
        ticket: ContentResolutionTicket,
        context: WorldContext,
        transition_id: Option<TransitionId>,
        manifest: minimium_world::world::WorldManifest,
        expected_fingerprint: u64,
    ) -> Result<(), SessionFailure> {
        if !content.matches(ticket, manifest) {
            return Err(SessionFailure::new(
                "session.world_resolver_manifest_mismatch",
                "world-content resolver returned a stale or different manifest",
            ));
        }
        if content.map().fingerprint() != expected_fingerprint {
            return Err(SessionFailure::new(
                "session.world_fingerprint_mismatch",
                "resolved world map does not match the server fingerprint",
            ));
        }
        if !content
            .terrain_material_profile()
            .matches(manifest, expected_fingerprint)
        {
            return Err(SessionFailure::new(
                "session.world_material_profile_mismatch",
                "resolved terrain material profile does not match the world content identity",
            ));
        }
        let profile_conflicts = self
            .active_world_content
            .iter()
            .map(|bundle| &bundle.content)
            .chain(
                self.pending_world_content
                    .iter()
                    .map(|bundle| &bundle.content),
            )
            .any(|prior| {
                prior.manifest() == manifest
                    && prior.map().fingerprint() == expected_fingerprint
                    && (prior.terrain_material_profile() != content.terrain_material_profile()
                        || prior.resolved_relief_policy() != content.resolved_relief_policy())
            });
        if profile_conflicts {
            return Err(SessionFailure::new(
                "session.world_material_profile_conflict",
                "the same recognized world content resolved with a conflicting material profile",
            ));
        }
        if let Some(planet) = manifest.planet().filter(|planet| planet.mode == 1) {
            let geography = content.geography().ok_or_else(|| {
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
        if ticket.context != context || ticket.transition_id != transition_id {
            return Err(SessionFailure::new(
                "session.world_resolver_context_mismatch",
                "world-content resolver returned content for a stale context or transition",
            ));
        }
        let content = Arc::new(content);
        if let Some(transition_id) = transition_id {
            self.pending_world_content = Some(PendingWorldContent {
                context,
                transition_id,
                ticket,
                content,
                water_catalog,
            });
        } else {
            self.active_world_content = Some(ActiveWorldContent {
                context,
                content,
                water_catalog,
            });
        }
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
        self.pump_world_content_job();
        if let Err(error) = self.poll_content() {
            self.log_failure(&error);
            self.disconnect(DisconnectReason::Materials(error.with_retry_hint()), events);
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

        self.pump_transition_readiness();
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
            && self.pending_ping_frame.is_none()
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

    pub(super) fn disconnect(&mut self, reason: DisconnectReason, events: &mut Vec<SessionEvent>) {
        self.clear_replicas();
        self.close();
        events.push(SessionEvent::Disconnected(reason));
    }
}
