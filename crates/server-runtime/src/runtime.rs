mod admission;
mod altitude;
mod bolid;
mod clock;
mod diagnostics;
mod environment;
mod inputs;
mod r#loop;
mod network;
mod operations;
mod outbound;
pub mod schedule;
mod tick;
use diagnostics::emit_system_effect_diagnostic;

use crate::server_command::ServerCommand;
use crate::{
    admin::{AdminServer, RequestId},
    composition::ServerCombatConfig,
    config::ServerConfig,
    connection::Client,
    environment::EnvironmentClock,
    executor::Executor,
    lifecycle::{self, EndpointGuard},
    metrics::RuntimeMetrics,
    sector_streaming::SectorGenerationPool,
    sector_streaming::SectorPayload,
    startup::Startup,
    storage::{PersistenceWorker, PlayerStore, SAVE_VERSION},
    terminal,
    world_inputs::WorldInput,
    world_registry::WorldRegistry,
    CLIENT_IDLE_TIMEOUT, MAX_PENDING_HANDSHAKES, MAX_PENDING_WORLD_INPUTS, SNAPSHOT_INTERVAL, TICK,
};
use minimium_core::{identity::PlayerProfileId, ids::PlayerId};
use minimium_environment::time::MonotonicMs;
use minimium_platform::{endpoint::ServerEndpoint, server_lock::ServerDataLock};
use minimium_protocol::ClientMessage;
use minimium_simulation::{CollisionQueryStats, CombatSettings, MovementIntent};
use minimium_world::world::WorldMap;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
    net::{SocketAddr, TcpListener},
    path::PathBuf,
    sync::{mpsc::Receiver, Arc},
    thread,
    time::{Duration, Instant},
};

struct GameState {
    worlds: WorldRegistry,
    sectors: minimium_sectors::SectorManager<SectorPayload>,
    active_sector_tickets: HashMap<PlayerId, Vec<minimium_sectors::SectorTicket>>,
    transfer_sector_tickets: HashMap<PlayerId, Vec<minimium_sectors::SectorTicket>>,
    pending_world_inputs: Vec<(WorldInput, Option<Instant>)>,
    gameplay_events: Vec<crate::modules::GameplayEvent>,
    pending_departures: HashSet<PlayerProfileId>,
    active_inputs: HashMap<PlayerId, MovementIntent>,
    player_movement_policies: HashMap<PlayerId, minimium_simulation::MovementPolicy>,
    input_last_received: HashMap<PlayerId, Instant>,
    pending_movement_inputs:
        HashMap<PlayerId, std::collections::BTreeMap<u64, minimium_protocol::MovementSample>>,
    last_input_seq: HashMap<PlayerId, u64>,
    pending_input_seqs: HashMap<PlayerId, std::collections::BTreeSet<u64>>,
    resolved_input_seq: HashMap<PlayerId, u64>,
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
    simulation_tick: u64,
}
pub(crate) struct ServerRuntime {
    pub(crate) server_logger: minimium_diagnostics::DomainLogger,
    shutdown_request: crate::composition::ShutdownRequest,
    pub(crate) transport_logger: minimium_diagnostics::DomainLogger,
    pub(crate) environment_logger: minimium_diagnostics::DomainLogger,
    pub(crate) protocol_logger: minimium_diagnostics::DomainLogger,
    pub(crate) world_logger: minimium_diagnostics::DomainLogger,
    pub(crate) sectors_logger: minimium_diagnostics::DomainLogger,
    pub(crate) simulation_logger: minimium_diagnostics::DomainLogger,
    content: crate::content::ContentServer,
    climate: minimium_environment::climate::ClimateScenario,
    authentication_provider: crate::authentication::SharedAuthenticationProvider,
    next_authentication_attempt: u64,
    executor: Executor,
    schedule: schedule::ServerSchedule,
    module_registry:
        crate::modules::ModuleRegistry<crate::commands::ModuleHandler, schedule::SystemFn>,
    component_registry: minimium_world::entity::ComponentRegistry,
    game: GameState,
    sector_workers: SectorGenerationPool,
    environment_clock: EnvironmentClock,
    environment_dirty_revision: Option<minimium_environment::time::ClockRevision>,
    connections: ConnectionState,
    operations: OperationsState,
    scheduler: Scheduler,
    metrics: RuntimeMetrics,
    map: WorldMap,
    prepared_geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    game_listener: Option<TcpListener>,
    game_address: SocketAddr,
    endpoint: ServerEndpoint,
    data_dir: PathBuf,
    _log_dir: PathBuf,
    max_players: usize,
    max_client_connections: usize,
    lanes: usize,
    combat: CombatSettings,
    movement_policy: minimium_simulation::MovementPolicy,
    shot_cooldown: Duration,
    autosave_seconds: u64,
    resolved_settings: std::collections::BTreeMap<String, minimium_commands::ResolvedConfigValue>,
    autosave_session_override: bool,
    bolid_interval: Option<(u64, u64)>,
    bolid_session_override: bool,
    next_bolid_at: Option<Instant>,
    next_bolid_id: u64,
    sky_views: HashMap<PlayerId, (minimium_protocol::WorldContext, [f32; 3], bool, Instant)>,
    last_manual_bolid_at: Option<Instant>,
    bolid_shower: Option<bolid::shower::Shower>,
    // Endpoint must be removed while the data lock is still held.
    _endpoint_guard: EndpointGuard,
    _data_lock: ServerDataLock,
}

impl ServerRuntime {
    pub(crate) fn new(
        config: ServerConfig,
        startup: Startup,
        combat: ServerCombatConfig,
        server_logger: minimium_diagnostics::DomainLogger,
        transport_logger: minimium_diagnostics::DomainLogger,
        environment_logger: minimium_diagnostics::DomainLogger,
        protocol_logger: minimium_diagnostics::DomainLogger,
        world_logger: minimium_diagnostics::DomainLogger,
        sectors_logger: minimium_diagnostics::DomainLogger,
        simulation_logger: minimium_diagnostics::DomainLogger,
        asset_io_logger: minimium_diagnostics::DomainLogger,
        authentication_provider: crate::authentication::SharedAuthenticationProvider,
        shutdown_request: crate::composition::ShutdownRequest,
        module_registry: crate::modules::ModuleRegistry<
            crate::commands::ModuleHandler,
            schedule::SystemFn,
        >,
        component_registry: minimium_world::entity::ComponentRegistry,
    ) -> io::Result<Self> {
        let climate_config = crate::environment::ClimateConfiguration::load(&config)?;
        let content = crate::content::ContentServer::new(&config, asset_io_logger)?;
        climate_config.commit()?;
        let climate = climate_config.scenario;
        let sector_workers = SectorGenerationPool::new(config.sector_workers)?;
        let lanes = config.lanes.count();
        let resolved_settings = config.resolved_settings.clone();
        let Startup {
            map,
            worlds,
            mut player_store,
            entity_counters,
            world_initialization,
            entities,
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
            prepared_geography,
        } = startup;
        let console_rx = if config.console_enabled {
            println!("Type `help` for server commands.");
            Some(terminal::spawn(server_logger.clone())?)
        } else {
            None
        };
        let executor = Executor::new(lanes);
        let mut worlds = worlds
            .map(WorldRegistry::try_from_world_definitions)
            .transpose()
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("generated world registry is invalid: {error:?}"),
                )
            })?
            .unwrap_or_else(|| WorldRegistry::authored_primary(map.clone()));
        let saved_sector_states = player_store.sector_states();
        worlds
            .restore_sector_states(saved_sector_states, player_store.snapshot_budget_mut())
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("saved sector state is incompatible with loaded worlds: {error:?}"),
                )
            })?;
        worlds
            .restore_persistent_entities(
                &entity_counters,
                &world_initialization,
                &entities,
                &component_registry,
            )
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("saved entities are incompatible with registered schemas: {error:?}"),
                )
            })?;
        let max_client_connections = config.max_players + MAX_PENDING_HANDSHAKES;
        let mut game = GameState {
            worlds,
            sectors: minimium_sectors::SectorManager::new(32_768, 32_768)
                .expect("sector cache and queue capacities are valid"),
            active_sector_tickets: HashMap::with_capacity(config.max_players),
            transfer_sector_tickets: HashMap::with_capacity(config.max_players),
            pending_world_inputs: Vec::new(),
            gameplay_events: Vec::with_capacity(MAX_PENDING_WORLD_INPUTS),
            pending_departures: HashSet::new(),
            active_inputs: HashMap::with_capacity(config.max_players),
            player_movement_policies: HashMap::with_capacity(config.max_players),
            input_last_received: HashMap::with_capacity(config.max_players),
            pending_movement_inputs: HashMap::with_capacity(config.max_players),
            last_input_seq: HashMap::with_capacity(config.max_players),
            pending_input_seqs: HashMap::with_capacity(config.max_players),
            resolved_input_seq: HashMap::with_capacity(config.max_players),
            last_global_chat_at: None,
        };
        let movement_policy = minimium_simulation::MovementPolicy::default();
        for (_, instance) in game.worlds.iter_mut() {
            instance.world.set_movement_policy(movement_policy);
        }
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
        let environment_clock = EnvironmentClock::new(
            environment_config,
            saved_clock,
            server_started,
            &environment_logger,
        )
        .map_err(io::Error::other)?;
        let environment_dirty_revision = environment_clock.dirty_revision();
        let schedule = crate::commands::build_schedule(&module_registry).map_err(|error| {
            io::Error::other(format!("gameplay system schedule is invalid: {error}"))
        })?;
        let autosave_interval = Duration::from_secs(config.autosave_seconds);
        let mut runtime = Self {
            server_logger,
            shutdown_request,
            transport_logger,
            environment_logger,
            protocol_logger,
            world_logger,
            sectors_logger,
            simulation_logger,
            game,
            sector_workers,
            content,
            climate,
            authentication_provider,
            next_authentication_attempt: 1,
            schedule,
            module_registry,
            component_registry,
            environment_clock,
            environment_dirty_revision,
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
                simulation_tick: 0,
            },
            metrics,
            executor,
            map,
            prepared_geography,
            game_listener: Some(game_listener),
            game_address,
            endpoint,
            data_dir: config.data_dir,
            _log_dir: config.log_dir,
            max_players: config.max_players,
            max_client_connections,
            lanes,
            combat: combat.hitscan,
            movement_policy,
            shot_cooldown: Duration::from_millis(combat.fire_cooldown_ms),
            autosave_seconds: config.autosave_seconds,
            resolved_settings,
            autosave_session_override: false,
            bolid_interval: config.bolid_interval,
            bolid_session_override: false,
            next_bolid_at: config
                .bolid_interval
                .map(|(minimum, _)| server_started + Duration::from_secs(minimum)),
            next_bolid_id: 1,
            sky_views: HashMap::new(),
            last_manual_bolid_at: None,
            bolid_shower: None,
            _endpoint_guard: endpoint_guard,
            _data_lock: data_lock,
        };
        runtime.run_system_stage(crate::modules::Stage::Startup)?;
        Ok(runtime)
    }

    fn run_system_stage(&mut self, stage: crate::modules::Stage) -> io::Result<()> {
        if stage == crate::modules::Stage::Startup {
            let startup_worlds: Vec<_> = self
                .game
                .worlds
                .iter()
                .map(|(instance, owner)| {
                    let lifecycle = if owner.content_initialized {
                        crate::modules::WorldLifecycle::Restored
                    } else {
                        crate::modules::WorldLifecycle::New
                    };
                    (instance, lifecycle)
                })
                .collect();
            for (instance, lifecycle) in startup_worlds {
                let batch = self.schedule.run_stage(
                    stage,
                    self.scheduler.simulation_tick,
                    &self.game.worlds,
                    &self.game.gameplay_events,
                    Some((instance, lifecycle)),
                );
                if batch.rejected > 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "startup system effect limit exceeded",
                    ));
                }
                self.commit_system_batch(batch)?;
            }
            self.game.worlds.initialize_new_content();
            return Ok(());
        }
        let batch = self.schedule.run_stage(
            stage,
            self.scheduler.simulation_tick,
            &self.game.worlds,
            &self.game.gameplay_events,
            None,
        );
        if batch.rejected > 0 {
            emit_system_effect_diagnostic(
                &self.world_logger,
                "buffer",
                &format!("{} effects rejected by stage limits", batch.rejected),
            );
            return Ok(());
        }
        self.commit_system_batch(batch)
    }

    fn commit_system_batch(&mut self, batch: crate::modules::WorldEffectBatch) -> io::Result<()> {
        let mut transfers = Vec::new();
        for effect in &batch.effects {
            if let crate::modules::WorldEffect::RequestTransfer {
                player,
                source_world,
                target_world,
                destination,
            } = effect
            {
                let context = self
                    .game
                    .worlds
                    .context_for_player(*player)
                    .filter(|context| context.instance() == *source_world)
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "gameplay transfer request has stale world context",
                        )
                    })?;
                transfers.push((
                    *player,
                    context,
                    *target_world,
                    *destination,
                    self.operations.player_store.return_location_for(*player),
                ));
            }
        }
        if !self.game.worlds.validate_transfer_batch(&transfers) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "gameplay transfer batch is invalid",
            ));
        }
        let entity_effects: Vec<_> = batch
            .effects
            .iter()
            .filter(|effect| !matches!(effect, crate::modules::WorldEffect::RequestTransfer { .. }))
            .cloned()
            .collect();
        if let Err(error) = self.game.worlds.apply_entity_effect_batch(
            &entity_effects,
            self.operations.player_store.snapshot_budget_mut(),
            &self.component_registry,
        ) {
            emit_system_effect_diagnostic(&self.world_logger, "stage_batch", &format!("{error:?}"));
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "gameplay effect batch rejected",
            ));
        }
        for (player, context, target_world, destination, _) in transfers {
            self.begin_entity_transfer(player, context, target_world, destination);
        }
        Ok(())
    }
}
