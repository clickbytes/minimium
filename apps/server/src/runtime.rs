mod admission;
mod inputs;
mod network;
mod operations;
mod outbound;
mod tick;

use crate::{
    admin::{AdminServer, RequestId},
    config::ServerConfig,
    connection::Client,
    environment::EnvironmentClock,
    executor::Executor,
    lifecycle::{self, EndpointGuard},
    metrics::RuntimeMetrics,
    sector_streaming::SectorPayload,
    startup::Startup,
    storage::{PersistenceWorker, PlayerStore, SAVE_VERSION},
    terminal,
    world_inputs::WorldInput,
    world_registry::WorldRegistry,
    CLIENT_HANDSHAKE_TIMEOUT, CLIENT_IDLE_TIMEOUT, MAX_PENDING_HANDSHAKES, SNAPSHOT_INTERVAL, TICK,
};
use minimium_core::{
    identity::PlayerProfileId,
    ids::PlayerId,
    time::{ClockRevision, MonotonicMs},
    world::{CharacterInput, CollisionQueryStats, WorldMap},
};
use minimium_platform::{
    commands::ServerCommand, endpoint::ServerEndpoint, server_lock::ServerDataLock,
};
use minimium_protocol::ClientMessage;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    sync::mpsc::Receiver,
    thread,
    time::{Duration, Instant},
};

struct GameState {
    worlds: WorldRegistry,
    sectors: minimium_sectors::SectorManager<SectorPayload>,
    active_sector_tickets: HashMap<PlayerId, Vec<minimium_sectors::SectorTicket>>,
    transfer_sector_tickets: HashMap<PlayerId, Vec<minimium_sectors::SectorTicket>>,
    pending_world_inputs: Vec<(WorldInput, Option<Instant>)>,
    pending_departures: HashSet<PlayerProfileId>,
    active_inputs: HashMap<PlayerId, CharacterInput>,
    input_last_received: HashMap<PlayerId, Instant>,
    last_global_chat_at: Option<Instant>,
}
struct ConnectionState {
    clients: Vec<Client>,
    client_indices_by_player: HashMap<PlayerId, usize>,
    first_write_client: usize,
    messages: Vec<(usize, ClientMessage, Instant)>,
    disconnected: Vec<usize>,
}
struct OperationsState {
    admin: AdminServer,
    console_rx: Option<Receiver<ServerCommand>>,
    command_queue: VecDeque<(Option<RequestId>, ServerCommand)>,
    player_store: PlayerStore,
    persistence: PersistenceWorker,
    next_player: u64,
}
struct Scheduler {
    server_started: Instant,
    next_utc_check: Instant,
    next_tick: Instant,
    next_snapshot: Instant,
    next_autosave: Option<Instant>,
    autosave_interval: Duration,
}
pub(crate) struct ServerRuntime {
    executor: Executor,
    game: GameState,
    environment_clock: EnvironmentClock,
    connections: ConnectionState,
    operations: OperationsState,
    scheduler: Scheduler,
    metrics: RuntimeMetrics,
    map: WorldMap,
    game_listener: TcpListener,
    game_address: SocketAddr,
    endpoint: ServerEndpoint,
    data_dir: PathBuf,
    max_players: usize,
    max_client_connections: usize,
    lanes: usize,
    #[allow(dead_code)]
    environment_dirty_revision: Option<ClockRevision>,
    autosave_seconds: u64,
    stop: bool,
    // Endpoint must be removed while the data lock is still held.
    _endpoint_guard: EndpointGuard,
    _data_lock: ServerDataLock,
}

impl ServerRuntime {
    pub(crate) fn new(config: ServerConfig, startup: Startup) -> io::Result<Self> {
        let lanes = config.lanes.count();
        let Startup {
            map,
            generated_worlds,
            player_store,
            persistence,
            next_player,
            game_listener,
            game_address,
            endpoint,
            endpoint_guard,
            data_lock,
            admin,
            environment_config,
            saved_clock,
        } = startup;
        println!("Minimium server started: game={}:{} admin=127.0.0.1:{} players={} lanes={lanes} autosave={}s map={:016x}", config.bind_address, endpoint.game_port, endpoint.admin_port, config.max_players, config.autosave_seconds, map.fingerprint());
        println!(
            "Admin CLI: minimiumctl --data-dir {} status",
            config.data_dir.display()
        );
        let console_rx = if config.console_enabled {
            println!("Type `help` for server commands.");
            Some(terminal::spawn()?)
        } else {
            None
        };
        let executor = Executor::new(lanes);
        let mut worlds = generated_worlds
            .map(WorldRegistry::from_generated_worlds)
            .unwrap_or_else(|| WorldRegistry::legacy_primary(map.clone()));
        worlds
            .restore_sector_states(player_store.sector_states())
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("saved sector state is incompatible with loaded worlds: {error:?}"),
                )
            })?;
        let max_client_connections = config.max_players + MAX_PENDING_HANDSHAKES;
        let game = GameState {
            worlds,
            sectors: minimium_sectors::SectorManager::new(32_768, 32_768)
                .expect("sector cache and queue capacities are valid"),
            active_sector_tickets: HashMap::with_capacity(config.max_players),
            transfer_sector_tickets: HashMap::with_capacity(config.max_players),
            pending_world_inputs: Vec::new(),
            pending_departures: HashSet::new(),
            active_inputs: HashMap::with_capacity(config.max_players),
            input_last_received: HashMap::with_capacity(config.max_players),
            last_global_chat_at: None,
        };
        let connections = ConnectionState {
            clients: Vec::with_capacity(max_client_connections),
            client_indices_by_player: HashMap::with_capacity(config.max_players),
            first_write_client: 0,
            messages: Vec::with_capacity(max_client_connections),
            disconnected: Vec::with_capacity(max_client_connections),
        };
        let operations = OperationsState {
            admin,
            console_rx,
            command_queue: VecDeque::with_capacity(64),
            player_store,
            persistence,
            next_player,
        };
        let metrics = RuntimeMetrics::default();
        let server_started = Instant::now();
        let environment_clock =
            EnvironmentClock::new(environment_config, saved_clock, server_started).map_err(
                |error| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("environment clock: {error}"),
                    )
                },
            )?;
        let autosave_interval = Duration::from_secs(config.autosave_seconds);
        Ok(Self {
            game,
            environment_clock,
            connections,
            operations,
            scheduler: Scheduler {
                server_started,
                next_utc_check: server_started + Duration::from_secs(5),
                next_tick: server_started + TICK,
                next_snapshot: Instant::now() + SNAPSHOT_INTERVAL,
                next_autosave: (!autosave_interval.is_zero())
                    .then_some(server_started + autosave_interval),
                autosave_interval,
            },
            metrics,
            executor,
            map,
            game_listener,
            game_address,
            endpoint,
            data_dir: config.data_dir,
            max_players: config.max_players,
            max_client_connections,
            lanes,
            environment_dirty_revision: None,
            autosave_seconds: config.autosave_seconds,
            stop: false,
            _endpoint_guard: endpoint_guard,
            _data_lock: data_lock,
        })
    }

    pub(crate) fn run(mut self) -> io::Result<()> {
        let mut runtime_error = None;
        while !self.stop {
            let loop_started = match self.poll_operations() {
                Ok(started) => started,
                Err(error) => {
                    runtime_error = Some(error);
                    break;
                }
            };
            if let Err(error) = self.poll_network() {
                runtime_error = Some(error);
                break;
            }
            self.tick();
            self.replicate();
            self.metrics.loop_work.record(loop_started.elapsed());
            self.plan_sleep();
        }
        lifecycle::shutdown(lifecycle::ShutdownContext {
            admin: &mut self.operations.admin,
            clients: &mut self.connections.clients,
            player_store: &mut self.operations.player_store,
            persistence: &mut self.operations.persistence,
            environment_clock: &mut self.environment_clock,
            server_started: self.scheduler.server_started,
            worlds: &self.game.worlds,
            map_fingerprint: self.map.fingerprint(),
            endpoint: &self.endpoint,
            data_dir: &self.data_dir,
            runtime_error,
        })
    }

    pub(super) fn monotonic_ms(&self, now: Instant) -> io::Result<MonotonicMs> {
        let elapsed = now.saturating_duration_since(self.scheduler.server_started);
        let millis = u64::try_from(elapsed.as_millis()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "server monotonic time overflow")
        })?;
        Ok(MonotonicMs::new(millis))
    }

    fn plan_sleep(&self) {
        let poll_interval = if self.connections.clients.is_empty() {
            Duration::from_millis(50)
        } else {
            Duration::from_millis(8)
        };
        let now = Instant::now();
        let until_tick = self.scheduler.next_tick.saturating_duration_since(now);
        let until_snapshot = self.scheduler.next_snapshot.saturating_duration_since(now);
        let until_autosave = self
            .scheduler
            .next_autosave
            .map(|deadline| deadline.saturating_duration_since(now))
            .unwrap_or(Duration::MAX);
        thread::sleep(
            poll_interval
                .min(until_tick)
                .min(until_snapshot)
                .min(until_autosave),
        );
    }
}
