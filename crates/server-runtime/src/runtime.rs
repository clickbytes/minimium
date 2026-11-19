mod action_state;
mod admission;
mod altitude;
mod bolid;
pub use bolid::receipts::{BolidShowerStatus, ShowerState};
mod clock;
mod combat;
mod component_ownership;
mod diagnostics;
mod effect_receipts;
mod entity_effects;
mod environment;
mod equipment;
mod excavation;
mod gameplay_terrain;
mod impacts;
pub(crate) use component_ownership::ComponentOwnerAuthority;
pub use component_ownership::{ComponentOwnerError, ComponentOwnerPort};
pub use effect_receipts::StartupEffectFailure;
pub use excavation::{
    ExcavationStatus, WaterJobState, WaterJobStatus, WaterStatus, WaterStatusError,
    WaterStatusState,
};
pub use impacts::{ImpactReceipt, ImpactReceiptStage, ImpactStatus};
mod hosted;
mod input_movement;
mod input_receipts;
mod input_transfer;
mod inputs;
mod r#loop;
mod movement_receipts;
mod network;
mod operations;
mod outbound;
pub mod schedule;
mod service_inputs;
mod stage;
mod tick;
mod transfers;
use diagnostics::emit_system_effect_diagnostic;
use hosted::{validate_hosted_inputs, CommandOrigin};

use crate::server_command::ServerCommand;
use crate::{
    admin::RequestId,
    composition::ServerCombatConfig,
    connection::Client,
    environment::EnvironmentClock,
    executor::Executor,
    lifecycle,
    metrics::RuntimeMetrics,
    sector_streaming::SectorGenerationPool,
    sector_streaming::SectorPayload,
    startup::PreparedRuntime,
    storage::{PersistencePort, PlayerStore, SAVE_VERSION},
    world_inputs::WorldInput,
    world_registry::WorldRegistry,
    CLIENT_IDLE_TIMEOUT, MAX_PENDING_HANDSHAKES, MAX_PENDING_WORLD_INPUTS, SNAPSHOT_INTERVAL, TICK,
};
use minimium_core::{identity::PlayerProfileId, ids::PlayerId};
use minimium_environment::time::MonotonicMs;
use minimium_protocol::ClientMessage;
use minimium_simulation::{CollisionQueryStats, CombatSettings, MovementIntent};
use minimium_world::world::WorldMap;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io,
    sync::{mpsc::Receiver, Arc},
    time::{Duration, Instant},
};

struct GameState {
    worlds: WorldRegistry,
    sectors: minimium_sectors::SectorManager<SectorPayload>,
    active_sector_tickets: HashMap<PlayerId, Vec<minimium_sectors::SectorTicket>>,
    transfer_sector_tickets: HashMap<PlayerId, Vec<minimium_sectors::SectorTicket>>,
    combat_sector_demands: HashMap<PlayerId, crate::sector_streaming::CombatSectorDemand>,
    combat_sector_tickets: HashMap<PlayerId, Vec<minimium_sectors::SectorTicket>>,
    pending_world_inputs: Vec<(WorldInput, Option<Instant>)>,
    gameplay_events: Vec<crate::modules::GameplayEvent>,
    pending_departures: HashSet<PlayerProfileId>,
    jumps: HashMap<PlayerId, Vec<movement_receipts::Ticket>>,
    active_inputs: HashMap<PlayerId, MovementIntent>,
    player_movement_policies: HashMap<PlayerId, minimium_simulation::MovementPolicy>,
    input_last_received: HashMap<PlayerId, Instant>,
    pending_movement_inputs: HashMap<PlayerId, input_movement::ScheduledMotion>,
    pending_grant_receipts: HashMap<
        PlayerId,
        (
            minimium_protocol::WorldContext,
            u64,
            minimium_world::items::Inventory,
        ),
    >,
    pending_portal_receipts: HashMap<PlayerId, (minimium_protocol::WorldContext, u64)>,
    pending_portal_transitions: HashMap<PlayerId, minimium_world::world::TransitionId>,
    last_input_seq: HashMap<PlayerId, u64>,
    pending_input_seqs: HashMap<PlayerId, std::collections::BTreeSet<u64>>,
    applied_input: HashMap<PlayerId, (u64, u64)>,
    resolved_input_seq: HashMap<PlayerId, u64>,
    actor_action_cooldowns: HashMap<PlayerId, action_state::ActorActionCooldowns>,
    fire_cooldown: Duration,
    last_global_chat_at: Option<Instant>,
}
struct ConnectionState {
    clients: Vec<Client>,
    client_indices_by_player: HashMap<PlayerId, usize>,
    messages: Vec<(usize, ClientMessage, Instant)>,
    disconnected: Vec<usize>,
    admin_requests: VecDeque<(RequestId, minimium_transport::AdminInput)>,
}
struct OperationsState {
    transport: Box<dyn minimium_transport::TransportPort>,
    console_rx: Option<Receiver<ServerCommand>>,
    command_queue: VecDeque<(CommandOrigin, ServerCommand)>,
    next_submission: u64,
    host_outcomes: Vec<crate::CommandExecution>,
    player_store: PlayerStore,
    persistence: Box<dyn PersistencePort>,
    next_player: u64,
}
struct Scheduler {
    server_started: Instant,
    pending: Option<clock::PendingStep>,
    transfers: Vec<crate::world_registry::CommittedTransfer>,
    demand: clock::StepDemand,
    next_snapshot: Instant,
    next_autosave: Option<Instant>,
    autosave_interval: Duration,
}
pub struct ServerRuntime {
    operation_catalog: Arc<minimium_commands::operation::OperationCatalog>,
    now: Instant,
    equipment: equipment::Equipment,
    excavation: excavation::Excavation,
    impacts: impacts::Impacts,
    pub(super) component_owner_authority: component_ownership::ComponentOwnerAuthority,
    pub(crate) server_logger: minimium_diagnostics::DomainLogger,
    shutdown_request: crate::composition::ShutdownRequest,
    pub(crate) transport_logger: minimium_diagnostics::DomainLogger,
    pub(crate) environment_logger: minimium_diagnostics::DomainLogger,
    pub(crate) protocol_logger: minimium_diagnostics::DomainLogger,
    pub(crate) world_logger: minimium_diagnostics::DomainLogger,
    pub(crate) sectors_logger: minimium_diagnostics::DomainLogger,
    pub(crate) simulation_logger: minimium_diagnostics::DomainLogger,
    content: Box<dyn crate::composition::ContentServicePort>,
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
    game_address: String,
    endpoint: Option<minimium_platform::endpoint::ServerEndpoint>,
    process_lifecycle: Option<Box<dyn crate::composition::ProcessLifecyclePort>>,
    max_players: usize,
    max_client_connections: usize,
    lanes: usize,
    combat: CombatSettings,
    movement_policy: minimium_simulation::MovementPolicy,
    autosave_seconds: u64,
    startup_autosave_seconds: u64,
    resolved_settings: std::collections::BTreeMap<String, minimium_commands::ResolvedConfigValue>,
    autosave_session_override: bool,
    bolid_interval: Option<(u64, u64)>,
    startup_bolid_interval: Option<(u64, u64)>,
    bolid_session_override: bool,
    next_bolid_at: Option<Duration>,
    next_bolid_id: u64,
    sky_views: HashMap<
        PlayerId,
        (
            minimium_protocol::WorldContext,
            [f32; 3],
            minimium_protocol::SkyObservation,
            Instant,
        ),
    >,
    last_manual_bolid_at: Option<Instant>,
    bolid_shower: Option<bolid::shower::Shower>,
    bolid_history: std::collections::VecDeque<bolid::receipts::BolidShowerStatus>,
}

impl ServerRuntime {
    pub fn new(
        config: crate::composition::RuntimeSettings,
        startup: PreparedRuntime,
        composition: crate::RuntimeComposition,
        execution: crate::RuntimeExecution,
        time: crate::RuntimeTime,
    ) -> io::Result<Self> {
        let crate::RuntimeComposition {
            combat,
            impacts: impact_profile,
            excavation: excavation_config,
            async_work_policy,
            modules,
            loggers,
            authentication: authentication_provider,
            shutdown: shutdown_request,
            session_nonce,
        } = composition;
        let crate::RuntimeLoggers {
            server: server_logger,
            transport: transport_logger,
            environment: environment_logger,
            protocol: protocol_logger,
            world: world_logger,
            sectors: sectors_logger,
            simulation: simulation_logger,
        } = loggers;
        validate_hosted_inputs(&config, &startup, &combat)?;
        if time
            .monotonic
            .checked_add(Duration::from_secs(604830))
            .is_none()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "host clock cannot represent runtime deadlines",
            ));
        }
        if !excavation_config.valid() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid excavation or fluid capacity configuration",
            ));
        }
        let crate::RuntimeExecution {
            executor,
            sectors: sector_workers,
            console: console_rx,
        } = execution;
        let lanes = executor.lane_count();
        let resolved_settings = config.resolved_settings.clone();
        let PreparedRuntime {
            map,
            primary_terrain_material_profile,
            worlds,
            mut player_store,
            entity_counters,
            world_initialization,
            entities,
            persistence,
            excavation_journal,
            mut excavation_restore,
            impact_journal,
            impact_restore,
            climate,
            content,
            next_player,
            transport,
            game_address,
            endpoint,
            process_lifecycle,
            environment_config,
            saved_clock,
            prepared_geography,
        } = startup;
        if player_store.excavation_revision() != excavation_restore.last_mutation_id {
            return Err(io::Error::new(io::ErrorKind::InvalidData,
                "excavation journal and world snapshot are from different simulation boundaries; restore the matching files"));
        }
        let equipment = equipment::Equipment::from_restored(
            std::mem::take(&mut excavation_restore.inventories),
            config.max_players,
        );
        let mut worlds = worlds
            .map(WorldRegistry::try_from_world_definitions)
            .transpose()
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("generated world registry is invalid: {error:?}"),
                )
            })?
            .unwrap_or_else(|| {
                WorldRegistry::authored_primary(map.clone(), primary_terrain_material_profile)
            });
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
                &modules.components,
            )
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("saved entities are incompatible with registered schemas: {error:?}"),
                )
            })?;
        hosted::reserve_restored_entities(&mut player_store, &entities)?;
        let max_client_connections = config.max_players + MAX_PENDING_HANDSHAKES;
        let mut game = GameState {
            worlds,
            sectors: minimium_sectors::SectorManager::new(32_768, 32_768)
                .expect("sector cache and queue capacities are valid"),
            active_sector_tickets: HashMap::with_capacity(config.max_players),
            transfer_sector_tickets: HashMap::with_capacity(config.max_players),
            combat_sector_demands: HashMap::with_capacity(config.max_players),
            combat_sector_tickets: HashMap::with_capacity(config.max_players),
            pending_world_inputs: Vec::new(),
            gameplay_events: Vec::with_capacity(MAX_PENDING_WORLD_INPUTS),
            pending_departures: HashSet::new(),
            jumps: HashMap::new(),
            active_inputs: HashMap::with_capacity(config.max_players),
            player_movement_policies: HashMap::with_capacity(config.max_players),
            input_last_received: HashMap::with_capacity(config.max_players),
            pending_movement_inputs: HashMap::with_capacity(config.max_players),
            pending_grant_receipts: HashMap::with_capacity(config.max_players),
            pending_portal_receipts: HashMap::with_capacity(config.max_players),
            pending_portal_transitions: HashMap::with_capacity(config.max_players),
            last_input_seq: HashMap::with_capacity(config.max_players),
            pending_input_seqs: HashMap::with_capacity(config.max_players),
            applied_input: HashMap::with_capacity(config.max_players),
            resolved_input_seq: HashMap::with_capacity(config.max_players),
            actor_action_cooldowns: HashMap::with_capacity(config.max_players),
            fire_cooldown: Duration::from_millis(combat.fire_cooldown_ms),
            last_global_chat_at: None,
        };
        let movement_policy = minimium_simulation::MovementPolicy::default();
        for (_, instance) in game.worlds.iter_mut() {
            instance.world.set_movement_policy(movement_policy);
        }
        let connections = ConnectionState {
            clients: Vec::with_capacity(max_client_connections),
            client_indices_by_player: HashMap::with_capacity(config.max_players),
            messages: Vec::with_capacity(max_client_connections),
            disconnected: Vec::with_capacity(max_client_connections),
            admin_requests: VecDeque::with_capacity(64),
        };
        let operations = OperationsState {
            next_submission: 1,
            host_outcomes: Vec::with_capacity(64),
            transport,
            console_rx,
            command_queue: VecDeque::with_capacity(64),
            player_store,
            persistence,
            next_player,
        };
        let metrics = RuntimeMetrics::default();
        let server_started = time.monotonic;
        let environment_clock = EnvironmentClock::new(
            environment_config,
            saved_clock,
            time.utc,
            session_nonce,
            &environment_logger,
        )
        .map_err(io::Error::other)?;
        let environment_dirty_revision = environment_clock.dirty_revision();
        let autosave_interval = Duration::from_secs(config.autosave_seconds);
        let component_owner_authority = component_ownership::ComponentOwnerAuthority::new(
            game.worlds.iter().map(|(instance, _)| instance),
        );
        let mut runtime = Self {
            now: time.monotonic,
            equipment,
            excavation: excavation::Excavation::from_storage(
                excavation_journal,
                excavation_restore,
                excavation_config,
                async_work_policy,
                time.monotonic,
            )?,
            impacts: impacts::Impacts::from_storage(
                impact_journal,
                impact_restore,
                impact_profile,
                async_work_policy,
            )?,
            component_owner_authority,
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
            schedule: modules.schedule,
            operation_catalog: modules.catalog,
            module_registry: modules.registry,
            component_registry: modules.components,
            environment_clock,
            environment_dirty_revision,
            connections,
            operations,
            scheduler: Scheduler {
                server_started,
                pending: None,
                transfers: Vec::new(),
                demand: clock::StepDemand::new(server_started),
                next_snapshot: server_started + SNAPSHOT_INTERVAL,
                next_autosave: (!autosave_interval.is_zero())
                    .then_some(server_started + autosave_interval),
                autosave_interval,
            },
            metrics,
            executor,
            map,
            prepared_geography,
            game_address,
            endpoint,
            process_lifecycle,
            max_players: config.max_players,
            max_client_connections,
            lanes,
            combat: combat.hitscan,
            movement_policy,
            autosave_seconds: config.autosave_seconds,
            startup_autosave_seconds: config.autosave_seconds,
            resolved_settings,
            autosave_session_override: false,
            bolid_interval: config.bolid_interval,
            startup_bolid_interval: config.bolid_interval,
            bolid_session_override: false,
            next_bolid_at: config.bolid_interval.map(|(minimum, _)| {
                Duration::from_millis(
                    saved_clock.map_or(0, |clock| clock.boundary().simulation_ms()),
                ) + Duration::from_secs(minimum)
            }),
            next_bolid_id: 1,
            sky_views: HashMap::new(),
            last_manual_bolid_at: None,
            bolid_shower: None,
            bolid_history: std::collections::VecDeque::new(),
        };
        runtime.initialize_component_owner_ports()?;
        runtime.restore_excavation_impacts()?;
        runtime.restore_impacts()?;
        runtime.restore_excavation()?;
        runtime.run_system_stage(crate::modules::Stage::Startup)?;
        Ok(runtime)
    }
}
