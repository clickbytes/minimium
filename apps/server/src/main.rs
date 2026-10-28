use minimium_core::ids::PlayerId;
mod accept;
mod admin;
mod commands;
mod config;
mod connection;
mod lifecycle;
mod message_dispatch;
mod metrics;
mod network;
mod receive;
mod replication;
mod send;
mod startup;
mod storage;
mod terminal;
mod world_inputs;
use config::{DEFAULT_AUTOSAVE_SECONDS, MAX_PLAYERS};
use connection::{Client, CLIENT_CLOSE_TIMEOUT};
use message_dispatch::DispatchContext;
use metrics::RuntimeMetrics;
use minimium_telemetry::allocations::{self, Phase};
use storage::{PersistenceWorker, PlayerStore, SAVE_VERSION};
use world_inputs::WorldInput;
#[cfg(feature = "allocation-metrics")]
#[global_allocator]
static ALLOCATOR: allocations::CountingAllocator = allocations::CountingAllocator;
use minimium_core::{
    executor::Executor,
    identity::PlayerProfileId,
    world::{CharacterInput, World},
};
use minimium_platform::commands::ServerCommand;
use minimium_protocol::{ClientMessage, PROTOCOL_VERSION};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    env, thread,
    time::{Duration, Instant},
};

const TICK: Duration = Duration::from_millis(50);
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(100);
const MAX_PENDING_HANDSHAKES: usize = 64;
const MAX_PENDING_WORLD_INPUTS: usize = 4096;
const CLIENT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const CLIENT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const SHOT_COOLDOWN: Duration = Duration::from_millis(250);
const CHAT_COOLDOWN: Duration = Duration::from_secs(1);
const GLOBAL_CHAT_COOLDOWN: Duration = Duration::from_millis(50);

fn main() {
    if let Err(error) = run() {
        eprintln!("server error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("minimium-server run [--threads 1..64|auto] [--max-players N] [--bind IP] [--game-port PORT] [--data-dir PATH] [--map PATH] [--autosave-seconds N] [--headless]\nminimium-server --version\n--max-players defaults to {MAX_PLAYERS}; --game-port 0 selects an available port (default); --autosave-seconds 0 disables periodic saves (default: {DEFAULT_AUTOSAVE_SECONDS}); --headless disables stdin commands; administer with minimiumctl.");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("minimium-server {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let config::ServerConfig {
        lanes,
        data_dir,
        map_path,
        bind_address,
        game_port,
        max_players,
        autosave_seconds,
        console_enabled,
    } = config::ServerConfig::parse(&args)?;
    let lanes = lanes.count();
    let startup::Startup {
        data_lock: _data_lock,
        map,
        mut player_store,
        mut persistence,
        mut next_player,
        game_listener,
        game_address,
        endpoint,
        endpoint_guard: _endpoint_guard,
        mut admin,
    } = startup::initialize(
        &data_dir,
        map_path.as_deref(),
        &bind_address,
        game_port,
        max_players,
    )?;
    println!(
        "Minimium server started: game={bind_address}:{} admin=127.0.0.1:{} players={max_players} lanes={lanes} autosave={autosave_seconds}s map={:016x}",
        endpoint.game_port,
        endpoint.admin_port,
        map.fingerprint()
    );
    println!(
        "Admin CLI: minimiumctl --data-dir {} status",
        data_dir.display()
    );
    if console_enabled {
        println!("Type `help` for server commands.");
    }

    let console_rx = if console_enabled {
        Some(terminal::spawn()?)
    } else {
        None
    };
    // This is the single dispatch queue. Terminal's bounded adapter may hold at most 64 more.
    let mut command_queue: VecDeque<(Option<admin::RequestId>, ServerCommand)> =
        VecDeque::with_capacity(64);
    let executor = Executor::new(lanes);
    let mut world = World::default();
    world.load_map(&map);
    let max_client_connections = max_players + MAX_PENDING_HANDSHAKES;
    let mut clients: Vec<Client> = Vec::with_capacity(max_client_connections);
    let mut first_write_client = 0usize;
    let mut client_indices_by_player: HashMap<PlayerId, usize> =
        HashMap::with_capacity(max_players);
    // A disconnected profile cannot rejoin until its queued Leave is applied.
    // The set is bounded by the reserved pending-world-input capacity.
    let mut pending_departures: HashSet<PlayerProfileId> = HashSet::new();
    let mut active_inputs: HashMap<PlayerId, CharacterInput> = HashMap::with_capacity(max_players);
    let mut input_last_received: HashMap<PlayerId, Instant> = HashMap::with_capacity(max_players);
    let mut pending_world_inputs = Vec::new();
    let mut messages: Vec<(usize, ClientMessage, Instant)> =
        Vec::with_capacity(max_client_connections);
    let mut disconnected: Vec<usize> = Vec::with_capacity(max_client_connections);
    let mut stop = false;
    let mut metrics = RuntimeMetrics::default();
    let server_started = Instant::now();
    let mut next_tick = server_started + TICK;
    let autosave_interval = Duration::from_secs(autosave_seconds);
    let mut next_autosave =
        (!autosave_interval.is_zero()).then_some(server_started + autosave_interval);
    let mut last_global_chat_at: Option<Instant> = None;
    let mut next_snapshot = Instant::now() + SNAPSHOT_INTERVAL;

    let mut runtime_error = None;
    'server: while !stop {
        persistence.poll_completions();
        let autosave_check_at = Instant::now();
        if next_autosave.is_some_and(|deadline| autosave_check_at >= deadline) {
            next_autosave = Some(autosave_check_at + autosave_interval);
            if persistence.pending_saves == 0 {
                if let Err(error) =
                    persistence.queue_world_save(&mut player_store, &world, map.fingerprint())
                {
                    eprintln!("could not queue automatic world save: {error}");
                } else {
                    println!("automatic world save queued");
                }
            } else {
                eprintln!("automatic world save skipped because another save is pending");
            }
        }
        metrics.sample_process_memory(Instant::now());
        messages.clear();
        disconnected.clear();
        let loop_started = Instant::now();
        let admin_requests = match admin.poll(Instant::now()) {
            Ok(requests) => requests,
            Err(error) => {
                runtime_error = Some(error);
                break 'server;
            }
        };
        for request in admin_requests {
            if command_queue.len() < 64 {
                command_queue.push_back((Some(request.id), request.command));
            } else {
                admin.respond(request.id, false, "command queue full");
            }
        }
        if let Some(console_rx) = &console_rx {
            for _ in 0..64 {
                let Ok(command) = console_rx.try_recv() else {
                    break;
                };
                if command_queue.len() < 64 {
                    command_queue.push_back((None, command));
                } else {
                    eprintln!("ERR command queue full");
                }
            }
        }

        let network_phase = allocations::enter(Phase::Network);
        let network_started = Instant::now();
        if let Err(error) = accept::accept_clients(
            &game_listener,
            &mut clients,
            max_client_connections,
            pending_world_inputs.len(),
        ) {
            runtime_error = Some(error);
            break 'server;
        }

        receive::poll_clients(&mut clients, &mut metrics, &mut messages, &mut disconnected);
        message_dispatch::dispatch_messages(DispatchContext {
            clients: &mut clients,
            disconnected: &mut disconnected,
            player_store: &mut player_store,
            max_players,
            pending_world_inputs: &mut pending_world_inputs,
            pending_departures: &pending_departures,
            next_player: &mut next_player,
            map: &map,
            client_indices_by_player: &mut client_indices_by_player,
            metrics: &mut metrics,
            last_global_chat_at: &mut last_global_chat_at,
            messages: &mut messages,
        });
        let eof_close_at = Instant::now();
        for client in &mut clients {
            if client.peer_eof && client.close_deadline.is_none() {
                client.close_deadline = Some(eof_close_at + CLIENT_CLOSE_TIMEOUT);
            }
        }
        let activity_check_at = Instant::now();
        for (index, client) in clients.iter().enumerate() {
            if client.close_deadline.is_some() {
                continue;
            }
            let handshake_timed_out = client.player_id.is_none()
                && activity_check_at.duration_since(client.connected_at)
                    >= CLIENT_HANDSHAKE_TIMEOUT;
            let idle_timed_out = client.player_id.is_some()
                && activity_check_at.duration_since(client.last_message_at) >= CLIENT_IDLE_TIMEOUT;
            if (handshake_timed_out || idle_timed_out) && !disconnected.contains(&index) {
                disconnected.push(index);
                if handshake_timed_out {
                    metrics.handshake_timeouts = metrics.handshake_timeouts.saturating_add(1);
                } else {
                    metrics.idle_timeouts = metrics.idle_timeouts.saturating_add(1);
                }
            }
        }
        send::send_clients(
            &mut clients,
            &mut metrics,
            &mut disconnected,
            &mut first_write_client,
        );
        metrics.oldest_output_age = clients
            .iter()
            .filter_map(|client| client.output_frames.front().map(|(_, at)| at.elapsed()))
            .max()
            .unwrap_or_default();
        disconnected.sort_unstable();
        disconnected.dedup();
        for index in disconnected.drain(..).rev() {
            if index < clients.len() {
                let client = clients.swap_remove(index);
                if let (Some(player_id), Some(profile_id)) = (client.player_id, client.profile_id) {
                    client_indices_by_player.remove(&player_id);
                    // Release the active claim immediately, but reject a rejoin
                    // until Leave has applied all preceding queued inputs.
                    if let Some(player) = world.player(player_id) {
                        if let Err(error) = player_store.update_from_player(player) {
                            eprintln!("could not capture disconnect state: {error}");
                        }
                    }
                    pending_departures.insert(profile_id);
                    player_store.release(player_id);
                    pending_world_inputs.push((WorldInput::Leave(player_id), None));
                    println!("player {player_id} disconnected bytes_in={} bytes_out={} decoded_frames={} queued_frames={} encoding_us={}", client.bytes_in, client.bytes_out, client.frames_in, client.frames_out, client.encoding_us);
                }
                if index < clients.len() {
                    if let Some(moved_player_id) = clients[index].player_id {
                        client_indices_by_player.insert(moved_player_id, index);
                    }
                }
            }
        }
        metrics.network_io.record(network_started.elapsed());
        metrics.max_pending_inputs = metrics.max_pending_inputs.max(pending_world_inputs.len());

        drop(network_phase);
        if Instant::now() >= next_tick {
            let tick_started = Instant::now();
            let command_phase = allocations::enter(Phase::Commands);
            for _ in 0..8 {
                let Some((request_id, command)) = command_queue.pop_front() else {
                    break;
                };
                if request_id.is_some_and(|id| !admin.is_pending(id, Instant::now())) {
                    continue;
                }
                let mut reply = commands::execute(
                    command.clone(),
                    commands::CommandContext {
                        world: &world,
                        player_store: &mut player_store,
                        persistence: &mut persistence,
                        map_fingerprint: map.fingerprint(),
                        lanes,
                        metrics: &metrics,
                    },
                );
                if command == ServerCommand::Status {
                    use std::fmt::Write as _;
                    let _ = write!(reply.text, " game_address={game_address} max_players={max_players} map_fingerprint={:016x} tick_ms={} snapshot_ms={} autosave_seconds={autosave_seconds} protocol_version={PROTOCOL_VERSION} save_version={SAVE_VERSION}", map.fingerprint(), TICK.as_millis(), SNAPSHOT_INTERVAL.as_millis());
                }
                if command == ServerCommand::Metrics {
                    use std::fmt::Write as _;
                    reply.text.push_str(" client_traffic=id:bytes_in:bytes_out:decoded_frames:queued_frames:encoding_us:connected_ms");
                    for client in &clients {
                        if let Some(id) = client.player_id {
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
                    admin.respond(id, reply.success, &reply.text);
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
                stop |= reply.stop;
                if stop {
                    break;
                }
            }
            drop(command_phase);
            let input_phase = allocations::enter(Phase::Input);
            let input_apply_started = Instant::now();
            world_inputs::apply_world_inputs(world_inputs::WorldInputsContext {
                pending_world_inputs: &mut pending_world_inputs,
                world: &mut world,
                active_inputs: &mut active_inputs,
                input_last_received: &mut input_last_received,
                player_store: &mut player_store,
                pending_departures: &mut pending_departures,
                client_indices_by_player: &client_indices_by_player,
                clients: &mut clients,
                metrics: &mut metrics,
            });
            world_inputs::expire_stale_inputs(
                &mut active_inputs,
                &input_last_received,
                Instant::now(),
            );
            metrics.input_apply.record(input_apply_started.elapsed());
            drop(input_phase);
            let simulation_phase = allocations::enter(Phase::Simulation);
            let simulation_started = Instant::now();
            let collision_stats =
                world.step_players_parallel(&active_inputs, TICK.as_secs_f32(), &executor);
            metrics.record_collision_queries(collision_stats);
            metrics.simulation.record(simulation_started.elapsed());
            drop(simulation_phase);
            for input in active_inputs.values_mut() {
                input.jump = false;
            }
            metrics.ticks = metrics.ticks.saturating_add(1);
            metrics.last_tick = tick_started.elapsed();
            metrics.tick.record(metrics.last_tick);
            next_tick += TICK;
            let now = Instant::now();
            if now > next_tick + TICK {
                let late_by = now.duration_since(next_tick).as_nanos();
                metrics.missed_ticks = metrics
                    .missed_ticks
                    .saturating_add((late_by / TICK.as_nanos()) as u64);
                next_tick = now + TICK;
            }
        }
        if Instant::now() >= next_snapshot {
            let _snapshot_phase = allocations::enter(Phase::Snapshot);
            let snapshot_started = Instant::now();
            let server_time_ms = server_started.elapsed().as_millis().min(u64::MAX as u128) as u64;
            replication::replicate_snapshots(
                &world,
                &executor,
                &mut clients,
                &mut metrics,
                server_time_ms,
            );
            metrics.snapshots = metrics.snapshots.saturating_add(1);
            metrics.snapshot_prep.record(snapshot_started.elapsed());
            replication::advance_deadline(&mut next_snapshot, SNAPSHOT_INTERVAL, &mut metrics);
        }
        metrics.loop_work.record(loop_started.elapsed());
        let poll_interval = if clients.is_empty() {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(8)
        };
        let now = Instant::now();
        let until_tick = next_tick.saturating_duration_since(now);
        let until_snapshot = next_snapshot.saturating_duration_since(now);
        let until_autosave = next_autosave
            .map(|deadline| deadline.saturating_duration_since(now))
            .unwrap_or(Duration::MAX);
        thread::sleep(
            poll_interval
                .min(until_tick)
                .min(until_snapshot)
                .min(until_autosave),
        );
    }

    lifecycle::shutdown(lifecycle::ShutdownContext {
        admin: &mut admin,
        clients: &mut clients,
        player_store: &mut player_store,
        persistence: &mut persistence,
        world: &world,
        map_fingerprint: map.fingerprint(),
        endpoint: &endpoint,
        data_dir: &data_dir,
        runtime_error,
    })?;
    Ok(())
}
