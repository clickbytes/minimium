use super::*;
use crate::{commands, replication, simulation, world_inputs};
use minimium_protocol::PROTOCOL_VERSION;
use minimium_telemetry::allocations::{self, Phase};

impl ServerRuntime {
    pub(super) fn tick(&mut self) {
        if Instant::now() >= self.scheduler.next_tick {
            let tick_started = Instant::now();
            self.apply_commands();
            self.apply_world_inputs();
            self.simulate_tick();
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
            let mut reply = commands::execute(
                command.clone(),
                commands::CommandContext {
                    world: &self.game.world,
                    player_store: &mut self.operations.player_store,
                    persistence: &mut self.operations.persistence,
                    map_fingerprint: self.map.fingerprint(),
                    lanes,
                    metrics: &self.metrics,
                },
            );
            if command == ServerCommand::Status {
                use std::fmt::Write as _;
                let _ = write!(reply.text, " game_address={game_address} max_players={max_players} map_fingerprint={:016x} tick_ms={} snapshot_ms={} autosave_seconds={autosave_seconds} protocol_version={PROTOCOL_VERSION} save_version={SAVE_VERSION}", self.map.fingerprint(), TICK.as_millis(), SNAPSHOT_INTERVAL.as_millis());
            }
            if command == ServerCommand::Metrics {
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
            if matches!(command, ServerCommand::Save | ServerCommand::Stop) {
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
        world_inputs::apply_world_inputs(world_inputs::WorldInputsContext {
            pending_world_inputs: &mut self.game.pending_world_inputs,
            world: &mut self.game.world,
            active_inputs: &mut self.game.active_inputs,
            input_last_received: &mut self.game.input_last_received,
            player_store: &mut self.operations.player_store,
            pending_departures: &mut self.game.pending_departures,
            client_indices_by_player: &self.connections.client_indices_by_player,
            clients: &mut self.connections.clients,
            metrics: &mut self.metrics,
        });
        world_inputs::expire_stale_inputs(
            &mut self.game.active_inputs,
            &self.game.input_last_received,
            Instant::now(),
        );
        self.metrics
            .input_apply
            .record(input_apply_started.elapsed());
        drop(input_phase);
    }
    fn simulate_tick(&mut self) {
        let simulation_phase = allocations::enter(Phase::Simulation);
        let simulation_started = Instant::now();
        let collision_stats = simulation::step_players(
            &mut self.game.world,
            &self.game.active_inputs,
            TICK.as_secs_f32(),
            &self.executor,
        );
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
                &self.game.world,
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
