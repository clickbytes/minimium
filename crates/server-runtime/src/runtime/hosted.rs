use super::*;
use crate::hosting::*;
use minimium_environment::time::SimulationLimit;
use minimium_gameplay::ModuleCommandReadView;

#[derive(Clone, Copy)]
pub(super) enum CommandOrigin {
    Console,
    Admin(RequestId, Option<minimium_commands::admin::Correlation>),
    Host(SubmissionId),
}

impl ServerRuntime {
    pub fn submit(&mut self, command: RuntimeCommand) -> Result<SubmissionId, SubmitError> {
        if self.shutdown_request.is_requested() {
            return Err(SubmitError::Stopping);
        }
        if self.operations.command_queue.len() >= 64 {
            return Err(SubmitError::QueueFull);
        }
        let next = self
            .operations
            .next_submission
            .checked_add(1)
            .ok_or(SubmitError::IdentifierExhausted)?;
        let id = SubmissionId(self.operations.next_submission);
        self.enqueue_command(CommandOrigin::Host(id), command.0)?;
        self.operations.next_submission = next;
        Ok(id)
    }

    pub(super) fn enqueue_command(
        &mut self,
        origin: CommandOrigin,
        command: ServerCommand,
    ) -> Result<(), SubmitError> {
        if self.shutdown_request.is_requested() {
            return Err(SubmitError::Stopping);
        }
        if self.operations.command_queue.len() >= 64 {
            return Err(SubmitError::QueueFull);
        }
        self.operations.command_queue.push_back((origin, command));
        Ok(())
    }

    pub fn query(&self, query: RuntimeQuery) -> RuntimeQueryResult {
        let view = crate::commands::RuntimeCommandReadView::new(&self.game.worlds);
        match query {
            RuntimeQuery::Status => RuntimeQueryResult::Status(RuntimeStatus {
                simulation_tick: self.environment_clock.boundary().tick(),
                world_count: view.world_count(),
                queued_commands: self.operations.command_queue.len(),
                stopping: self.shutdown_request.is_requested(),
                persistence: self.operations.persistence.status(),
            }),
            RuntimeQuery::Worlds => RuntimeQueryResult::Worlds(
                view.worlds(
                    &crate::CommandViewRequest::new(
                        minimium_gameplay::MAX_COMMAND_VIEW_ROWS,
                        minimium_gameplay::MAX_COMMAND_VIEW_BYTES,
                    )
                    .expect("valid built-in page limits"),
                )
                .expect("valid built-in world page"),
            ),
            RuntimeQuery::WorldsPage(request) => {
                RuntimeQueryResult::WorldsPage(view.worlds(&request))
            }
            RuntimeQuery::World(id) => RuntimeQueryResult::World(view.world(id)),
            RuntimeQuery::Players => RuntimeQueryResult::Players(
                view.players(
                    &crate::CommandViewRequest::new(
                        minimium_gameplay::MAX_COMMAND_VIEW_ROWS,
                        minimium_gameplay::MAX_COMMAND_VIEW_BYTES,
                    )
                    .expect("valid built-in page limits"),
                )
                .expect("valid built-in player page"),
            ),
            RuntimeQuery::PlayersPage(request) => {
                RuntimeQueryResult::PlayersPage(view.players(&request))
            }
            RuntimeQuery::Entities(world, request) => {
                RuntimeQueryResult::Entities(view.entities(world, &request))
            }
            RuntimeQuery::EntityRows(request) => {
                RuntimeQueryResult::EntityRows(view.entity_rows(&request))
            }
            RuntimeQuery::Components(world, entity, request) => RuntimeQueryResult::Components(
                view.components(world, entity, &request).map(|page| {
                    let rows = page
                        .rows
                        .into_iter()
                        .map(|component| minimium_gameplay::CommandComponentRow {
                            entity: component.entity,
                            kind: component.kind,
                            schema: component.schema,
                            bytes: component.bytes.to_vec(),
                        })
                        .collect();
                    crate::CommandViewPage::new(rows, page.next)
                }),
            ),
            RuntimeQuery::ComponentMetadata(world, entity, request) => {
                RuntimeQueryResult::ComponentMetadata(
                    view.component_metadata(world, entity, &request),
                )
            }
        }
    }

    /// Services host deadlines, then runs a bounded batch of complete fixed steps.
    pub fn step(&mut self, time: RuntimeTime) -> io::Result<StepOutcome> {
        if time.monotonic < self.now
            || time
                .monotonic
                .checked_add(Duration::from_secs(604830))
                .is_none()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "runtime time moved backwards or cannot represent deadlines",
            ));
        }
        self.now = time.monotonic;
        self.environment_clock.set_host_utc(time.utc);
        let previous_tick = self.environment_clock.boundary().tick();
        if !self.shutdown_request.is_requested() {
            let result = (|| -> io::Result<()> {
                let dropped = self
                    .scheduler
                    .demand
                    .accrue(self.now, self.environment_clock.rate().get());
                self.metrics.missed_ticks = self.metrics.missed_ticks.saturating_add(dropped);
                if dropped > 0 {
                    self.environment_clock.limit = SimulationLimit::Batch;
                }
                self.connections.disconnected.clear();
                self.poll_impact_journal();
                self.update_excavation();
                if self.worker_barrier() {
                    self.environment_clock.limit =
                        if self.impacts.failed() || self.excavation.failed() {
                            SimulationLimit::WorkerFailed
                        } else {
                            SimulationLimit::Worker
                        };
                }
                let started = self.poll_operations()?;
                if !self.shutdown_request.is_requested() {
                    self.poll_network()?;
                }
                if !self.shutdown_request.is_requested() {
                    self.apply_commands();
                    self.expire_pending_transfers(self.now);
                    self.service_inputs();
                    if !self.shutdown_request.is_requested() {
                        if dropped == 0 && !self.worker_barrier() {
                            self.environment_clock.limit = SimulationLimit::None;
                        }
                        self.tick();
                    }
                    if let Some(rate) = self.scheduler.demand.measured_rate(self.now) {
                        self.environment_clock.achieved_ppm = rate;
                    }
                }
                if !self.shutdown_request.is_requested() {
                    self.replicate_environment();
                    if self.scheduler.pending.is_none() {
                        self.replicate_impacts();
                        self.publish_excavation_stream();
                        self.replicate();
                    }
                }
                self.metrics.loop_work.record(started.elapsed());
                Ok(())
            })();
            if let Err(error) = result {
                self.shutdown_request.request();
                self.cancel_commands();
                return Err(error);
            }
        }
        if self.shutdown_request.is_requested() {
            self.cancel_commands();
        }
        Ok(StepOutcome {
            simulation_tick: self.environment_clock.boundary().tick(),
            tick_executed: self.environment_clock.boundary().tick() != previous_tick,
            stopping: self.shutdown_request.is_requested(),
            next_wake: self.next_wake(),
            commands: std::mem::take(&mut self.operations.host_outcomes),
        })
    }

    pub(super) fn simulation_work_pending(&self) -> bool {
        self.scheduler.pending.is_some() || self.worker_barrier()
    }

    pub(super) fn worker_barrier(&self) -> bool {
        self.excavation.busy()
            || self.excavation.failed()
            || self.impacts.pending_count() != 0
            || self.impacts.failed()
    }

    pub(super) fn cancel_commands(&mut self) {
        for (origin, command) in self.operations.command_queue.drain(..) {
            let reply = crate::commands::CommandReply::error(
                "server.stopping",
                "server stopping before command execution",
            );
            match origin {
                CommandOrigin::Host(id) => self.operations.host_outcomes.push(CommandExecution {
                    id,
                    identity: command.identity(id.get(), &self.operation_catalog),
                    reply,
                }),
                CommandOrigin::Admin(id, correlation) => {
                    let _ = self.operations.transport.respond_admin(
                        id,
                        crate::admin::response(
                            &reply,
                            correlation,
                            &command,
                            &self.operation_catalog,
                        ),
                    );
                }
                CommandOrigin::Console => {}
            }
        }
    }
}

pub(super) fn validate_hosted_inputs(
    config: &crate::RuntimeSettings,
    startup: &PreparedRuntime,
    combat: &ServerCombatConfig,
) -> io::Result<()> {
    if !(1..=crate::config::MAX_PLAYERS).contains(&config.max_players)
        || config.autosave_seconds > 604800
        || config
            .bolid_interval
            .is_some_and(|(low, high)| !(10..=3600).contains(&low) || !(low..=3600).contains(&high))
        || !combat.hitscan.range.is_finite()
        || combat.hitscan.range <= 0.0
        || combat.hitscan.range > 512.0
        || startup.next_player != startup.player_store.next_player_id()?
        || startup.game_address.len() > 1024
        || config.resolved_settings.len() > 256
        || config
            .resolved_settings
            .iter()
            .fold(0usize, |total, (key, value)| {
                total
                    .saturating_add(key.len())
                    .saturating_add(value.requested.len())
                    .saturating_add(value.default.as_ref().map_or(0, String::len))
            })
            > 64 * 1024
        || startup.entity_counters.len() > minimium_storage::MAX_SNAPSHOT_WORLDS
        || startup.world_initialization.len() > minimium_storage::MAX_SNAPSHOT_WORLDS
        || startup.entities.len() > minimium_world::entity::MAX_PERSISTENT_ENTITIES
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid hosted runtime settings or player identity",
        ));
    }
    startup.climate.validate().map_err(io::Error::other)?;
    validate_restored_metadata(startup)?;
    minimium_storage::excavation::validate_restore(&startup.excavation_restore)?;
    if let Some(first) = startup.worlds.as_ref().and_then(|worlds| worlds.first()) {
        if first.map.fingerprint() != startup.map.fingerprint()
            || first.terrain_material_profile.profile() != startup.primary_terrain_material_profile
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "primary world map/material profile mismatch",
            ));
        }
    }
    Ok(())
}

fn validate_restored_metadata(startup: &PreparedRuntime) -> io::Result<()> {
    use minimium_world::entity::{
        MAX_COMPONENTS_PER_ENTITY, MAX_COMPONENT_BYTES, MAX_COMPONENT_PAYLOAD_BYTES,
    };
    let invalid = || {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid or duplicate restored entity metadata",
        )
    };
    let counters: HashSet<_> = startup.entity_counters.iter().map(|(id, _)| *id).collect();
    let initialized: HashSet<_> = startup
        .world_initialization
        .iter()
        .map(|(id, _)| *id)
        .collect();
    if counters.len() != startup.entity_counters.len()
        || initialized.len() != startup.world_initialization.len()
    {
        return Err(invalid());
    }
    let mut identities = HashSet::new();
    let mut payload = 0usize;
    for entity in &startup.entities {
        if !identities.insert((entity.world, entity.id))
            || entity.components.len() > MAX_COMPONENTS_PER_ENTITY
        {
            return Err(invalid());
        }
        let mut kinds = HashSet::new();
        for component in &entity.components {
            payload = payload.saturating_add(component.bytes.len());
            if !kinds.insert(component.kind)
                || component.bytes.len() > MAX_COMPONENT_BYTES
                || payload > MAX_COMPONENT_PAYLOAD_BYTES
            {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

pub(super) fn reserve_restored_entities(
    store: &mut PlayerStore,
    entities: &[minimium_storage::PersistedEntity],
) -> io::Result<()> {
    use minimium_world::entity::{ComponentBlob, ComponentMutation, WorldEntityChange};
    // File loading already reserves these keys; replacement is idempotent. An
    // injected restore must acquire exactly the same storage budget before the
    // constructor can publish its completed owner.
    let mut changes = Vec::new();
    for entity in entities {
        changes.push(WorldEntityChange {
            world: entity.world,
            mutation: ComponentMutation::Spawn(entity.id),
        });
        for component in &entity.components {
            let blob =
                ComponentBlob::new(component.schema, component.bytes.clone()).ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "invalid restored component")
                })?;
            changes.push(WorldEntityChange {
                world: entity.world,
                mutation: ComponentMutation::Upsert(entity.id, component.kind, blob),
            });
        }
    }
    store
        .snapshot_budget_mut()
        .prepare_entity_changes(&changes)?
        .commit();
    Ok(())
}
