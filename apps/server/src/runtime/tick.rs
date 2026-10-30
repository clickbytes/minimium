use super::*;
use crate::{commands, replication, simulation};
use minimium_protocol::PROTOCOL_VERSION;
use minimium_telemetry::allocations::{self, Phase};

impl ServerRuntime {
    pub(super) fn tick(&mut self) {
        if Instant::now() >= self.scheduler.next_tick {
            let tick_started = Instant::now();
            self.apply_commands();
            self.apply_world_inputs();
            self.schedule.run_stage(
                minimium_platform::modules::Stage::FixedUpdate,
                &self.game.worlds,
            );
            self.simulate_tick();
            self.schedule.run_stage(
                minimium_platform::modules::Stage::Environment,
                &self.game.worlds,
            );
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
    fn apply_commands(&mut self) {
        let game_address = self.game_address;
        let max_players = self.max_players;
        let lanes = self.lanes;
        let autosave_seconds = self.autosave_seconds;
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
            let mut reply = match monotonic_ms {
                Ok(monotonic_ms) => commands::execute(
                    &command,
                    commands::CommandContext {
                        world: &self.game.worlds.primary().world,
                        worlds: &self.game.worlds,
                        player_store: &mut self.operations.player_store,
                        persistence: &mut self.operations.persistence,
                        map_fingerprint: self.map.fingerprint(),
                        lanes,
                        metrics: &self.metrics,
                        environment_clock: &mut self.environment_clock,
                        monotonic_ms,
                        schedule_diagnostics,
                    },
                ),
                Err(error) => commands::CommandReply {
                    success: false,
                    text: format!("monotonic clock: {error}"),
                    stop: false,
                },
            };
            self.environment_dirty_revision = self.environment_clock.dirty_revision();
            if command.is_status() {
                use std::fmt::Write as _;
                let _ = write!(reply.text, " game_address={game_address} max_players={max_players} map_fingerprint={:016x} tick_ms={} snapshot_ms={} autosave_seconds={autosave_seconds} protocol_version={PROTOCOL_VERSION} save_version={SAVE_VERSION}", self.map.fingerprint(), TICK.as_millis(), SNAPSHOT_INTERVAL.as_millis());
                reply.text.push_str(" environment_profiles=");
                reply.text.push_str(&self.game.worlds.environment_summary());
            }
            if command.is_metrics() {
                use std::fmt::Write as _;
                reply.text.push_str(" client_traffic=id:bytes_in:bytes_out:decoded_frames:queued_frames:encoding_us:connected_ms");
                for client in &self.connections.clients {
                    if let Some(id) = client.player_id() {
                        let _ = write!(
                            reply.text,
                            " {id}:{}:{}:{}:{}:{}:{}",
                            client.bytes_in,
                            client.bytes_out,
                            client.frames_in,
                            client.frames_out,
                            client.encoding_us,
                            client.connected_at.elapsed().as_millis()
                        );
                    }
                }
            }
            if let Some(id) = request_id {
                self.operations
                    .admin
                    .respond(id, reply.success, &reply.text);
            } else {
                println!(
                    "{} {}",
                    if reply.success { "OK" } else { "ERR" },
                    reply.text
                );
            }
            if command.is_save_or_stop() {
                println!("admin {:?}: {}", command, reply.text);
            }
            self.stop |= reply.stop;
            if self.stop {
                break;
            }
        }
        drop(command_phase);
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
        let committed = self.game.worlds.commit_ready_transfers(sector_ready);
        let mut committed_players = HashSet::with_capacity(committed.len());
        if !committed.is_empty() {
            self.scheduler.next_snapshot = Instant::now();
        }
        for transfer in committed {
            committed_players.insert(transfer.player);
            self.operations
                .player_store
                .set_return_location(transfer.player, transfer.source_location);
            self.game.active_inputs.insert(
                transfer.player,
                minimium_core::world::CharacterInput::default(),
            );
            self.game
                .input_last_received
                .insert(transfer.player, Instant::now());
            let Some(index) = self
                .connections
                .client_indices_by_player
                .get(&transfer.player)
                .copied()
            else {
                continue;
            };
            let (clock_sample, environment) = match self
                .environment_state_for_instance(transfer.context.instance(), Instant::now())
            {
                Ok(state) => state,
                Err(error) => {
                    eprintln!("could not sample committed environment: {error}");
                    if let Some(client) = self.connections.clients.get_mut(index) {
                        client.disconnect_reason =
                            Some(crate::connection::CloseReason::InvalidSequence);
                    }
                    self.connections.disconnected.push(index);
                    continue;
                }
            };
            let Some(client) = self.connections.clients.get_mut(index) else {
                continue;
            };
            client.replicated_players = minimium_protocol::RemotePlayers::default();
            client.replication_initialized = false;
            let queued = crate::connection::queue_message(
                client,
                &mut self.metrics,
                minimium_protocol::ServerMessage::TransitionCommitted {
                    transition_id: transfer.transition_id,
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
                client.mark_environment_sent(
                    transfer.context,
                    clock_sample.revision(),
                    Instant::now(),
                );
            }
        }
        committed_players
    }
    fn simulate_tick(&mut self) {
        let simulation_phase = allocations::enter(Phase::Simulation);
        let simulation_started = Instant::now();
        let ready_transfers = crate::sector_streaming::update(
            &mut self.game.worlds,
            &mut self.game.sectors,
            &mut self.game.active_sector_tickets,
            &mut self.game.transfer_sector_tickets,
        );
        let committed_players = self.commit_ready_transfers(&ready_transfers.transfers);
        let mut ready_players = ready_transfers.players;
        ready_players.extend(committed_players);
        let (worlds, active_inputs) = (&mut self.game.worlds, &self.game.active_inputs);
        let mut collision_stats = CollisionQueryStats::default();
        for (_, instance) in worlds.iter_mut() {
            let earth = instance
                .manifest
                .planet()
                .filter(|definition| definition.mode == 1);
            let previous = earth.map(|_| instance.world.snapshot());
            collision_stats.merge(simulation::step_ready_players(
                &mut instance.world,
                active_inputs,
                &ready_players,
                TICK.as_secs_f32(),
                &self.executor,
            ));
            if let (Some(definition), Some(previous)) = (earth, previous) {
                for player in previous {
                    if let Some(current) = instance.world.player(player.id) {
                        if !minimium_procgen::earth_is_land(
                            current.position,
                            definition.height_scale_permille,
                            definition.sea_level_mm,
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
        self.metrics.simulation.record(simulation_started.elapsed());
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
            replication::replicate_snapshots(
                &self.game.worlds,
                &self.executor,
                &mut self.connections.clients,
                &mut self.metrics,
                server_time_ms,
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
