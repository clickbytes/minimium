mod decode;

use super::*;

pub(super) type PreparedWorldContent = Result<
    (
        ResolvedWorldContent,
        Option<Arc<minimium_procgen::WaterCatalog>>,
    ),
    SessionFailure,
>;

pub(super) enum WorldContentProgress {
    Ready,
    Pending,
}

impl ClientSession {
    pub(super) fn prepare_world_content(
        &mut self,
        context: WorldContext,
        transition_id: Option<TransitionId>,
        manifest: minimium_world::world::WorldManifest,
        expected_fingerprint: u64,
    ) -> Result<WorldContentProgress, SessionFailure> {
        let existing_ticket = self.world_content_request.or_else(|| {
            self.pending_world_content
                .as_ref()
                .filter(|pending| {
                    pending.context == context && Some(pending.transition_id) == transition_id
                })
                .map(|pending| pending.ticket)
        });
        let ticket = match existing_ticket.filter(|ticket| {
            ticket.context == context
                && ticket.transition_id == transition_id
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
                    context,
                    transition_id,
                    content_revision: manifest.content_revision().get(),
                };
                self.world_content_request = Some(ticket);
                ticket
            }
        };
        if self
            .content_for_resolution_ticket(ticket)
            .is_some_and(|content| {
                content.manifest() == manifest
                    && content.map().fingerprint() == expected_fingerprint
            })
        {
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
            )
            .with_content_ticket(ticket));
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
            let result = worker
                .join()
                .map_err(|_| {
                    SessionFailure::new(
                        "session.world_content_worker_stopped",
                        "world-content worker stopped unexpectedly",
                    )
                })
                .map_err(|failure| {
                    failure.with_content_ticket(completed_ticket.unwrap_or(ticket))
                })?;
            if completed_ticket != Some(ticket) {
                return self.start_world_content_resolution(ticket, manifest, expected_fingerprint);
            }
            let (content, water_catalog) =
                result.map_err(|failure| failure.with_content_ticket(ticket))?;
            self.install_resolved_world_content(
                content,
                water_catalog,
                ticket,
                context,
                transition_id,
                manifest,
                expected_fingerprint,
            )
            .map_err(|failure| failure.with_content_ticket(ticket))?;
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
            .with_content_ticket(ticket)
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
                .spawn(move || {
                    let content = resolver.resolve(request, cancellation)?;
                    let terrain_inputs = minimium_procgen::TerrainInputs::new(
                        content.prepared_terrain().clone(),
                        minimium_procgen::TerrainOverlaySnapshot::empty_for(
                            ticket.context.instance(),
                        ),
                    )
                    .map_err(|error| {
                        SessionFailure::caused_by(
                            error.code(),
                            "world terrain source does not match its content ticket",
                            error,
                        )
                    })?;
                    let catalog = prepare_water_catalog(manifest, terrain_inputs)
                        .map_err(|error| {
                            SessionFailure::caused_by(
                                error.code(),
                                "world water catalog preparation failed",
                                error,
                            )
                        })?
                        .map(Arc::new);
                    Ok((content, catalog))
                })
                .map_err(|error| {
                    SessionFailure::caused_by(
                        "session.world_content_worker_start_failed",
                        "could not start world-content worker",
                        error,
                    )
                    .with_content_ticket(ticket)
                })?,
        );
        self.world_content_worker_ticket = Some(ticket);
        self.world_content_started = Some(Instant::now());
        Ok(WorldContentProgress::Pending)
    }

    fn install_resolved_world_content(
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

fn prepare_water_catalog(
    manifest: minimium_world::world::WorldManifest,
    inputs: minimium_procgen::TerrainInputs,
) -> Result<Option<minimium_procgen::WaterCatalog>, minimium_procgen::GenerationError> {
    if manifest.topology() != minimium_world::world::WorldTopology::Planet {
        return Ok(None);
    }
    let terrain = minimium_procgen::PlanetTerrain::for_manifest(manifest, inputs)?;
    minimium_procgen::WaterCatalog::build(
        &terrain,
        minimium_procgen::DEFAULT_WATER_CATALOG_RESOLUTION,
    )
    .map(Some)
}
