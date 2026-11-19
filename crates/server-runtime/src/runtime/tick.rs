use super::*;
use crate::{commands, replication, simulation};
use minimium_environment::time::SimulationLimit;
mod command_execution;
mod dispatch_commands;
mod properties;
mod status;
use minimium_telemetry::allocations::{self, Phase};

impl ServerRuntime {
    pub(super) fn tick(&mut self) {
        let started = Instant::now();
        for _ in 0..clock::MAX_STEPS {
            if self.scheduler.pending.is_none() {
                if !self.scheduler.demand.due() || self.shutdown_request.is_requested() {
                    return;
                }
                if started.elapsed() >= Duration::from_millis(4) {
                    self.environment_clock.limit = SimulationLimit::CpuBudget;
                    return;
                }
                if self.worker_barrier() {
                    self.environment_clock.limit =
                        if self.impacts.failed() || self.excavation.failed() {
                            SimulationLimit::WorkerFailed
                        } else {
                            SimulationLimit::Worker
                        };
                    return;
                }
                let Some((boundary, utc)) = self.environment_clock.next_boundary() else {
                    self.scheduler
                        .demand
                        .control_changed(self.environment_clock.rate().get());
                    self.service_inputs();
                    return;
                };
                if !self.prepare_water_boundary(boundary.tick()) {
                    self.environment_clock.limit = SimulationLimit::Water;
                    return;
                }
                self.scheduler.demand.consume();
                self.scheduler.pending = Some(clock::PendingStep {
                    boundary,
                    utc,
                    operation: 0,
                });
            }
            let tick_started = Instant::now();
            if !self.finish_pending_step() {
                return;
            }
            self.metrics.ticks = self.metrics.ticks.saturating_add(1);
            self.metrics.last_tick = tick_started.elapsed();
            self.metrics.tick.record(self.metrics.last_tick);
            // Deferred administration observes this exact boundary before more work.
            if !self.operations.command_queue.is_empty() {
                return;
            }
        }
    }

    pub(super) fn finish_pending_step(&mut self) -> bool {
        while let Some(pending) = self.scheduler.pending {
            if self.worker_barrier() {
                self.environment_clock.limit = if self.impacts.failed() || self.excavation.failed()
                {
                    SimulationLimit::WorkerFailed
                } else {
                    SimulationLimit::Worker
                };
                return false;
            }
            match self.schedule.tick_step(pending.operation) {
                Some(schedule::RuntimeScheduleStep::Operation(operation)) => {
                    if !self.run_tick_operation(operation) {
                        return false;
                    }
                }
                Some(schedule::RuntimeScheduleStep::Stage(stage)) => {
                    self.run_tick_system_stage(stage)
                }
                None => {
                    self.environment_clock.commit(pending.boundary, pending.utc);
                    self.scheduler.pending = None;
                    for transfer in std::mem::take(&mut self.scheduler.transfers) {
                        self.send_transition_committed(transfer);
                    }
                    self.scheduler.demand.finish();
                    return true;
                }
            }
            self.scheduler.pending.as_mut().unwrap().operation += 1;
        }
        true
    }

    fn run_tick_operation(
        &mut self,
        operation: crate::runtime::schedule::BuiltinOperation,
    ) -> bool {
        use crate::runtime::schedule::BuiltinOperation as Op;
        match operation {
            Op::Bolids => self.update_bolids(),
            Op::Impacts => return self.update_impacts(),

            Op::WorldInputs => self.apply_world_inputs(),
            Op::Simulation => self.simulate_tick(),
            Op::ClearEvents => self.game.gameplay_events.clear(),
        }
        true
    }

    fn run_tick_system_stage(&mut self, stage: crate::modules::Stage) {
        if let Err(error) = self.run_system_stage(stage) {
            crate::diagnostics::emit(
                &self.world_logger,
                minimium_diagnostics::Level::Error,
                "gameplay.stage_failed",
                &[
                    ("stage", format!("{stage:?}")),
                    ("error_kind", format!("{:?}", error.kind())),
                ],
            );
        }
    }
    fn apply_world_inputs(&mut self) {
        let input_phase = allocations::enter(Phase::Input);
        let input_apply_started = Instant::now();
        self.apply_inputs();
        self.metrics
            .input_apply
            .record(input_apply_started.elapsed());
        drop(input_phase);
    }

    fn commit_ready_transfers(
        &mut self,
        sector_ready: &HashSet<minimium_core::ids::PlayerId>,
    ) -> HashSet<minimium_core::ids::PlayerId> {
        let terrain =
            super::gameplay_terrain::build(&self.game.worlds, self.prepared_geography.as_ref());
        let reads: std::collections::HashMap<_, &dyn minimium_world::terrain::TerrainRead> =
            terrain
                .iter()
                .map(|(instance, read)| {
                    (*instance, read as &dyn minimium_world::terrain::TerrainRead)
                })
                .collect();
        let (committed, cancelled) =
            self.game
                .worlds
                .commit_ready_transfers_with_terrain(sector_ready, &reads, self.now);
        let mut committed_players = HashSet::with_capacity(committed.len());
        if !committed.is_empty() {
            self.scheduler.next_snapshot = self.now;
        }
        for transfer in committed {
            committed_players.insert(transfer.player);
            self.scheduler.transfers.push(transfer);
        }
        for cancelled in cancelled {
            self.finish_transfer_cancellation(cancelled);
        }
        committed_players
    }

    fn send_transition_committed(&mut self, transfer: crate::world_registry::CommittedTransfer) {
        self.operations
            .player_store
            .set_return_location(transfer.player, transfer.source_location);
        self.game.active_inputs.insert(
            transfer.player,
            minimium_simulation::MovementIntent::default(),
        );
        self.game
            .input_last_received
            .insert(transfer.player, self.now);
        self.game.pending_world_inputs.retain(|(input, _)| {
            !matches!(input, crate::world_inputs::WorldInput::Move(player, _) if *player == transfer.player)
        });
        self.game.pending_movement_inputs.remove(&transfer.player);
        self.game.pending_input_seqs.remove(&transfer.player);
        self.game.last_input_seq.insert(transfer.player, 0);
        self.game.resolved_input_seq.insert(transfer.player, 0);
        self.game.applied_input.remove(&transfer.player);
        self.sky_views.remove(&transfer.player);
        let Some(index) = self
            .connections
            .client_indices_by_player
            .get(&transfer.player)
            .copied()
        else {
            return;
        };
        let (clock_sample, environment) =
            match self.environment_state_for_instance(transfer.context.instance(), self.now) {
                Ok(state) => state,
                Err(error) => {
                    crate::diagnostics::emit(
                        &self.server_logger,
                        minimium_diagnostics::Level::Error,
                        "server.transfer_environment_failed",
                        &[
                            ("player_id", transfer.player.to_string()),
                            ("error_code", error.code().to_owned()),
                        ],
                    );
                    if let Some(client) = self.connections.clients.get_mut(index) {
                        client.disconnect_reason =
                            Some(crate::connection::CloseReason::InvalidSequence);
                    }
                    self.connections.disconnected.push(index);
                    return;
                }
            };
        let Some(client) = self.connections.clients.get_mut(index) else {
            return;
        };
        client.replicated_players = minimium_protocol::RemotePlayers::default();
        client.replication_initialized = false;
        client.entity_sync = None;
        client.entity_revision = 0;
        client.entity_sync_initialized = false;
        client.replicated_components.clear();
        let queued = crate::connection::queue_message(
            client,
            &mut self.metrics,
            minimium_protocol::ServerMessage::TransitionCommitted {
                transition_id: transfer.transition_id,
                server_tick: clock_sample.boundary().tick(),
                context: transfer.context,
                map_fingerprint: transfer.map_fingerprint,
                destination: transfer.destination,
                destination_mm: transfer.destination_mm,
                health: transfer.health,
                manifest: transfer.manifest,
                clock_sample,
                environment,
            },
        );
        if !queued {
            self.metrics.dropped_messages = self.metrics.dropped_messages.saturating_add(1);
            client.disconnect_reason = Some(crate::connection::CloseReason::Backpressure);
            self.connections.disconnected.push(index);
        } else {
            client.mark_environment_sent(transfer.context, clock_sample.revision(), self.now);
        }
    }
    fn simulate_tick(&mut self) {
        for (player, policy) in &self.game.player_movement_policies {
            self.game
                .worlds
                .set_player_movement_policy(*player, *policy);
        }
        let simulation_phase = allocations::enter(Phase::Simulation);
        let simulation_started = Instant::now();
        let ready_transfers = crate::sector_streaming::update_with_geography(
            &mut self.game.worlds,
            &mut self.game.sectors,
            &mut self.game.active_sector_tickets,
            &mut self.game.transfer_sector_tickets,
            &mut self.sector_workers,
            &mut self.game.combat_sector_demands,
            &mut self.game.combat_sector_tickets,
            self.prepared_geography.clone(),
            &self.sectors_logger,
        );
        let committed_players = self.commit_ready_transfers(&ready_transfers.transfers);
        let mut ready_players = ready_transfers.players;
        ready_players.extend(committed_players);
        ready_players.extend(
            self.game
                .active_inputs
                .iter()
                .filter(|(id, _)| {
                    self.game
                        .player_movement_policies
                        .get(id)
                        .is_some_and(|policy| policy.can_fly)
                })
                .map(|(id, _)| *id),
        );
        let (worlds, active_inputs) = (&mut self.game.worlds, &self.game.active_inputs);
        let mut collision_stats = CollisionQueryStats::default();
        for (_, instance) in worlds.iter_mut() {
            let manifest = instance.manifest;
            let has_surface_policy = manifest.planet().is_some_and(|planet| {
                manifest.topology() == minimium_world::world::WorldTopology::Planet
                    && planet.kind() == minimium_world::world::PlanetKind::Earth
            });
            let previous = has_surface_policy.then(|| instance.world.snapshot());
            collision_stats.merge(simulation::step_ready_players(
                &mut instance.world,
                active_inputs,
                &ready_players,
                TICK.as_secs_f32(),
                &self.executor,
            ));
            if let Some((batched, batches)) = instance.world.precise_dispatch_stats() {
                self.metrics.record_precise_dispatch(batched, batches);
            }
            if let Some(previous) = previous {
                for player in previous {
                    if active_inputs.get(&player.id).is_some_and(|_| {
                        self.game
                            .player_movement_policies
                            .get(&player.id)
                            .is_some_and(|policy| policy.can_fly)
                    }) {
                        continue;
                    }
                    if let Some(current) = instance.world.player(player.id) {
                        let position_mm = current.position_mm.unwrap_or_else(|| {
                            minimium_world::world::Position3Mm(
                                current
                                    .position
                                    .map(|value| (f64::from(value) * 1000.0).round() as i64),
                            )
                        });
                        if !minimium_procgen::can_traverse_surface(
                            manifest,
                            position_mm,
                            self.prepared_geography.as_deref(),
                        ) {
                            instance.world.set_position(player.id, player.position);
                            if let Some(position_mm) = player.position_mm {
                                instance.world.restore_position_mm(player.id, position_mm);
                            }
                        }
                    }
                }
            }
        }
        self.metrics.record_collision_queries(collision_stats);
        let simulation_elapsed = simulation_started.elapsed();
        self.metrics.simulation.record(simulation_elapsed);
        if simulation_elapsed > TICK {
            crate::diagnostics::emit(
                &self.simulation_logger,
                minimium_diagnostics::Level::Warn,
                "simulation.tick_budget_exceeded",
                &[("elapsed_us", simulation_elapsed.as_micros().to_string())],
            );
        }
        drop(simulation_phase);
        for input in self.game.active_inputs.values_mut() {
            input.jump = false;
        }
    }
    pub(super) fn replicate(&mut self) {
        if self.now >= self.scheduler.next_snapshot {
            let _snapshot_phase = allocations::enter(Phase::Snapshot);
            let snapshot_started = Instant::now();
            let server_time_ms = self.environment_clock.boundary().simulation_ms();
            replication::replicate_entities(
                &self.game.worlds,
                &self.component_registry,
                &self.game.active_sector_tickets,
                &mut self.connections.clients,
                &mut self.metrics,
                self.now,
            );
            replication::replicate_snapshots(
                &self.game.worlds,
                &self.executor,
                &mut self.connections.clients,
                &mut self.metrics,
                server_time_ms,
                self.environment_clock.boundary().tick(),
                &self.game.resolved_input_seq,
                &self.game.applied_input,
                &self.game.active_inputs,
            );
            self.metrics.snapshots = self.metrics.snapshots.saturating_add(1);
            self.metrics
                .snapshot_prep
                .record(snapshot_started.elapsed());
            replication::advance_deadline(
                &mut self.scheduler.next_snapshot,
                SNAPSHOT_INTERVAL,
                &mut self.metrics,
                self.now,
            );
        }
    }
}
