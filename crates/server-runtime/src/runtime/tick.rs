use super::*;
use crate::{commands, replication, simulation};
mod properties;
mod status;
use minimium_telemetry::allocations::{self, Phase};

impl ServerRuntime {
    pub(super) fn tick(&mut self) {
        if Instant::now() >= self.scheduler.next_tick {
            let tick_started = Instant::now();
            self.scheduler.simulation_tick = self.scheduler.simulation_tick.saturating_add(1);
            self.apply_commands();
            self.update_bolids();
            self.update_impacts();
            self.update_excavation();
            if self.shutdown_request.is_requested() {
                return;
            }
            self.apply_world_inputs();
            self.run_tick_system_stage(crate::modules::Stage::FixedUpdate);
            self.simulate_tick();
            self.run_tick_system_stage(crate::modules::Stage::Environment);
            self.run_tick_system_stage(crate::modules::Stage::PostSimulation);
            self.game.gameplay_events.clear();
            self.metrics.ticks = self.metrics.ticks.saturating_add(1);
            self.metrics.last_tick = tick_started.elapsed();
            self.metrics.tick.record(self.metrics.last_tick);
            self.scheduler.next_tick += TICK;
            let now = Instant::now();
            if now > self.scheduler.next_tick + TICK {
                let late_by = now.duration_since(self.scheduler.next_tick).as_nanos();
                self.metrics.missed_ticks = self
                    .metrics
                    .missed_ticks
                    .saturating_add((late_by / TICK.as_nanos()) as u64);
                self.scheduler.next_tick = now + TICK;
            }
        }
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
    fn apply_commands(&mut self) {
        let game_address = self.game_address;
        let max_players = self.max_players;
        let lanes = self.lanes;
        let command_phase = allocations::enter(Phase::Commands);
        for _ in 0..8 {
            let Some((request_id, command)) = self.operations.command_queue.pop_front() else {
                break;
            };
            if request_id.is_some_and(|id| !self.operations.admin.is_pending(id, Instant::now())) {
                continue;
            }
            let monotonic_ms = self.monotonic_ms(Instant::now());
            let schedule_diagnostics = self.schedule.diagnostics();
            let runtime_status = command
                .is_status()
                .then(|| self.runtime_status_snapshot(game_address, max_players));
            let client_traffic = if command.is_metrics() {
                self.client_traffic_snapshot()
            } else {
                commands::ClientTrafficSnapshot::default()
            };
            let reply = match &command {
                crate::server_command::ServerCommand::Registered(line) => {
                    self.execute_module_command(line)
                }
                crate::server_command::ServerCommand::SpawnRandomMeteorid => {
                    self.spawn_meteorid(true)
                }
                crate::server_command::ServerCommand::Property(property) => {
                    self.apply_property_command(property)
                }
                crate::server_command::ServerCommand::SpawnRandomBolid(count) => {
                    self.start_bolid_shower(*count)
                }
                _ => match monotonic_ms {
                    Ok(monotonic_ms) => commands::execute(
                        &command,
                        commands::OwnerCommandContext {
                            worlds: &self.game.worlds,
                            player_store: &mut self.operations.player_store,
                            persistence: &mut self.operations.persistence,
                            map_fingerprint: self.map.fingerprint(),
                            lanes,
                            metrics: &self.metrics,
                            environment_clock: &mut self.environment_clock,
                            monotonic_ms,
                            schedule_diagnostics,
                            runtime_status,
                            client_traffic,
                        },
                        &self.module_registry,
                    ),
                    Err(error) => commands::CommandReply::error(
                        "server.monotonic_clock_failed",
                        &format!("monotonic clock: {error}"),
                    ),
                },
            };
            self.environment_dirty_revision = self.environment_clock.dirty_revision();
            if !reply.is_success() {
                crate::diagnostics::emit(
                    &self.server_logger,
                    minimium_diagnostics::Level::Warn,
                    "server.command_execution_failed",
                    &[(
                        "error_code",
                        reply
                            .error_code()
                            .unwrap_or("server.command.failed")
                            .to_owned(),
                    )],
                );
            }
            if let Some(id) = request_id {
                self.operations.admin.respond_command(id, &reply);
            } else {
                let prefix = match reply.error_code() {
                    Some(code) => format!("ERR {code}"),
                    None => String::from("ERR"),
                };
                let text = crate::command_output::render_reply(&reply);
                println!(
                    "{} {}",
                    if reply.is_success() { "OK" } else { &prefix },
                    text.as_str()
                );
            }
            if command.is_save_or_stop() {
                crate::diagnostics::emit(
                    &self.server_logger,
                    minimium_diagnostics::Level::Info,
                    "server.admin_command_completed",
                    &[
                        (
                            "action",
                            (if matches!(command, crate::server_command::ServerCommand::Stop) {
                                "stop"
                            } else {
                                "save"
                            })
                            .to_owned(),
                        ),
                        ("success", reply.is_success().to_string()),
                    ],
                );
            }
            if reply.should_stop() {
                self.shutdown_request.request();
            }
            if self.shutdown_request.is_requested() {
                break;
            }
        }
        drop(command_phase);
    }

    fn execute_module_command(&mut self, line: &str) -> commands::CommandReply {
        let mut effects = crate::modules::WorldEffectBuffer::default();
        let mut reply = commands::execute_module_command(
            line,
            &self.game.worlds,
            &mut effects,
            &self.module_registry,
        );
        if reply.handler_succeeded() {
            let batch = effects.into_batch();
            if !batch.effects().is_empty() || batch.rejected_pushes() > 0 {
                match self.commit_effect_batch(batch) {
                    Ok(outcome) => {
                        self.report_effect_batch_outcome("command", &outcome);
                        reply.set_effect_outcome(outcome);
                    }
                    Err(error) => {
                        reply = commands::CommandReply::error(
                            "server.command.effect_commit_failed",
                            &format!("could not commit command effects: {error}"),
                        );
                    }
                }
            }
        }
        reply
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
        let (committed, cancelled) = self
            .game
            .worlds
            .commit_ready_transfers_with_terrain(sector_ready, &reads);
        let mut committed_players = HashSet::with_capacity(committed.len());
        if !committed.is_empty() {
            self.scheduler.next_snapshot = Instant::now();
        }
        for transfer in committed {
            committed_players.insert(transfer.player);
            self.send_transition_committed(transfer);
        }
        for (player, transition_id, context) in cancelled {
            let Some(index) = self
                .connections
                .client_indices_by_player
                .get(&player)
                .copied()
            else {
                continue;
            };
            if !crate::connection::queue_message(
                &mut self.connections.clients[index],
                &mut self.metrics,
                minimium_protocol::ServerMessage::TransitionCancelled {
                    transition_id,
                    context,
                },
            ) {
                self.metrics.dropped_messages = self.metrics.dropped_messages.saturating_add(1);
                self.connections.clients[index].disconnect_reason =
                    Some(crate::connection::CloseReason::Backpressure);
                self.connections.disconnected.push(index);
            }
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
            .insert(transfer.player, Instant::now());
        self.game.pending_world_inputs.retain(|(input, _)| {
            !matches!(input, crate::world_inputs::WorldInput::Move(player, _) if *player == transfer.player)
        });
        self.game.pending_movement_inputs.remove(&transfer.player);
        self.game.pending_input_seqs.remove(&transfer.player);
        self.game.last_input_seq.insert(transfer.player, 0);
        self.game.resolved_input_seq.insert(transfer.player, 0);
        let Some(index) = self
            .connections
            .client_indices_by_player
            .get(&transfer.player)
            .copied()
        else {
            return;
        };
        let (clock_sample, environment) = match self
            .environment_state_for_instance(transfer.context.instance(), Instant::now())
        {
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
                server_tick: self.scheduler.simulation_tick,
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
            client.mark_environment_sent(transfer.context, clock_sample.revision(), Instant::now());
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
        if Instant::now() >= self.scheduler.next_snapshot {
            let _snapshot_phase = allocations::enter(Phase::Snapshot);
            let snapshot_started = Instant::now();
            let server_time_ms = self
                .scheduler
                .server_started
                .elapsed()
                .as_millis()
                .min(u64::MAX as u128) as u64;
            replication::replicate_entities(
                &self.game.worlds,
                &self.component_registry,
                &self.game.active_sector_tickets,
                &mut self.connections.clients,
                &mut self.metrics,
            );
            replication::replicate_snapshots(
                &self.game.worlds,
                &self.executor,
                &mut self.connections.clients,
                &mut self.metrics,
                server_time_ms,
                self.scheduler.simulation_tick,
                &self.game.resolved_input_seq,
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
            );
        }
    }
}
