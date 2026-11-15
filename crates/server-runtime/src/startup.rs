use crate::{
    composition::{WorldComposer, WorldCompositionRequest, WorldDefinition},
    config::ServerConfig,
    content::ContentServer,
    environment::{self, ClimateConfiguration, EnvironmentConfig},
    storage::{FileStorageBackend, PersistencePort, PlayerStore, StorageBackend},
};
use minimium_environment::climate::ClimateScenario;
use minimium_environment::time::SavedClockState;
use minimium_platform::{
    admin_client::{SHUTDOWN_FILE, SHUTDOWN_OWNER_FILE},
    endpoint::ServerEndpoint,
    server_lock::ServerDataLock,
};
use minimium_world::world::{WorldInstanceId, WorldMap};
use std::{
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) struct EndpointGuard {
    data_dir: PathBuf,
    endpoint: ServerEndpoint,
    logger: minimium_diagnostics::DomainLogger,
}

impl Drop for EndpointGuard {
    fn drop(&mut self) {
        if let Err(error) = self.endpoint.remove_if_owned(&self.data_dir) {
            crate::diagnostics::emit(
                &self.logger,
                minimium_diagnostics::Level::Error,
                "server.endpoint_cleanup_failed",
                &[("error_kind", format!("{:?}", error.kind()))],
            );
        }
    }
}

fn write_shutdown_status(
    data_dir: &Path,
    endpoint: &ServerEndpoint,
    result: &io::Result<()>,
) -> io::Result<()> {
    use std::io::Write;
    let admin_port = endpoint.admin_port;
    let path = data_dir.join(SHUTDOWN_FILE);
    let temporary_path = data_dir.join("server.shutdown.tmp");
    let write_result = (|| {
        let mut file = File::create(&temporary_path)?;
        match result {
            Ok(()) => writeln!(file, "{admin_port}\tOK")?,
            Err(error) => writeln!(
                file,
                "{admin_port}\tERR\t{}",
                crate::storage::compact_error(error)
            )?,
        }
        file.sync_all()?;
        let owner_path = data_dir.join("server.shutdown.owner.tmp");
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut owner = options.open(owner_path)?;
        writeln!(owner, "{}", endpoint.token)?;
        owner.sync_all()?;
        drop(owner);
        minimium_platform::files::atomic_replace_file(
            &data_dir.join("server.shutdown.owner.tmp"),
            &data_dir.join(SHUTDOWN_OWNER_FILE),
        )?;
        minimium_platform::files::atomic_replace_file(&temporary_path, &path)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(temporary_path);
    }
    write_result
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub struct PreparedRuntime {
    pub map: WorldMap,
    pub primary_terrain_material_profile: minimium_procgen::TerrainMaterialProfile,
    pub worlds: Option<Vec<WorldDefinition>>,
    pub player_store: PlayerStore,
    pub entity_counters: Vec<(WorldInstanceId, u64)>,
    pub world_initialization: Vec<(WorldInstanceId, bool)>,
    pub entities: Vec<minimium_storage::PersistedEntity>,
    pub persistence: Box<dyn PersistencePort>,
    pub excavation_journal: Box<dyn minimium_storage::excavation::ExcavationJournalPort>,
    pub excavation_restore: minimium_storage::excavation::ExcavationRestore,
    pub impact_journal: Box<dyn minimium_storage::ImpactJournalPort>,
    pub impact_restore: Vec<minimium_world::impact::BodyState>,
    pub climate: ClimateScenario,
    pub content: Box<dyn crate::composition::ContentServicePort>,
    pub next_player: u64,
    pub transport: Box<dyn minimium_transport::TransportPort>,
    pub game_address: String,
    pub endpoint: Option<ServerEndpoint>,
    pub process_lifecycle: Option<Box<dyn crate::composition::ProcessLifecyclePort>>,
    pub environment_config: EnvironmentConfig,
    pub saved_clock: Option<SavedClockState>,
    pub prepared_geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
}

struct FileProcessLifecycle {
    data_dir: PathBuf,
    endpoint: ServerEndpoint,
    _endpoint_guard: EndpointGuard,
    _data_lock: ServerDataLock,
}

impl crate::composition::ProcessLifecyclePort for FileProcessLifecycle {
    fn remove_endpoint(&mut self) -> io::Result<()> {
        self.endpoint.remove_if_owned(&self.data_dir)
    }

    fn write_shutdown_status(&mut self, result: &io::Result<()>) -> io::Result<()> {
        write_shutdown_status(&self.data_dir, &self.endpoint, result)
    }
}

pub(crate) fn initialize(
    config: &ServerConfig,
    world_composer: WorldComposer,
    component_registry: &minimium_world::entity::ComponentRegistry,
    storage_logger: minimium_diagnostics::DomainLogger,
    server_logger: minimium_diagnostics::DomainLogger,
    asset_io_logger: minimium_diagnostics::DomainLogger,
    transport: Box<dyn minimium_transport::TransportPort>,
) -> Result<PreparedRuntime, Box<dyn std::error::Error>> {
    let data_dir = &config.data_dir;
    let storage_backend = FileStorageBackend::new(data_dir.clone(), storage_logger.clone());
    let requested_seed = config.seed;
    let map_path = config.map_path.as_deref();
    let world_definition_path = config.world_definition_path.as_deref();
    let planet_config_path = config.planet_config_path.as_deref();

    storage_backend.preflight()?;
    if config.environment_config_path.is_some()
        && ["world.seed", "world.planet.ron", "world.snapshot"]
            .iter()
            .any(|name| data_dir.join(name).exists())
    {
        return Err("--environment-config initializes a new world only; use time commands for an existing world".into());
    }
    let environment_config = config
        .environment_config_path
        .as_deref()
        .map(environment::load_config)
        .transpose()?
        .unwrap_or_default();
    let seed_path = data_dir.join("world.seed");
    let seed_bytes = read_optional_file(&seed_path, 64)?;
    let planet_path = data_dir.join("world.planet.ron");
    let planet_bytes = read_optional_file(&planet_path, 64 * 1024)?;
    let existing_seed = parse_seed(seed_bytes.as_deref())?;
    let composition = world_composer(WorldCompositionRequest {
        data_dir: data_dir.to_path_buf(),
        requested_seed,
        existing_seed,
        map_path: map_path.map(Path::to_path_buf),
        world_definition_path: world_definition_path.map(Path::to_path_buf),
        planet_config_path: planet_config_path.map(Path::to_path_buf),
    })?;
    if let Some(primary_world) = composition
        .worlds
        .as_ref()
        .and_then(|worlds| worlds.first())
    {
        if primary_world.map.fingerprint() != composition.primary_map.fingerprint()
            || primary_world.terrain_material_profile.profile()
                != composition.primary_terrain_material_profile
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "primary world map or material profile differs from its recognized definition",
            )
            .into());
        }
    }
    let seed = composition.seed;
    let map = composition.primary_map;
    let generated_worlds = composition.worlds;
    let planet_definition = composition.planet_definition;
    let prepared_geography = composition.prepared_geography;
    let primary_terrain_material_profile = composition.primary_terrain_material_profile;

    let primary = WorldInstanceId::new(1).expect("primary instance identifier is nonzero");
    let world_maps = generated_worlds
        .as_ref()
        .map(|worlds| {
            worlds
                .iter()
                .map(|world| (world.instance, world.map.clone(), world.manifest))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![(
                primary,
                map.clone(),
                authored_manifest(1, map.fingerprint()),
            )]
        });
    // Reject incompatible saved worlds before creating the process lock file.
    drop(storage_backend.load(&world_maps, component_registry)?);
    let data_lock = ServerDataLock::acquire(data_dir)?;
    storage_backend.preflight()?;
    if config.environment_config_path.is_some()
        && ["world.seed", "world.planet.ron", "world.snapshot"]
            .iter()
            .any(|name| data_dir.join(name).exists())
    {
        return Err("--environment-config initializes a new world only; use time commands for an existing world".into());
    }
    if read_optional_file(&seed_path, 64)? != seed_bytes
        || read_optional_file(&planet_path, 64 * 1024)? != planet_bytes
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "world seed or planet definition changed during startup preflight",
        )
        .into());
    }
    let loaded_state = storage_backend.load(&world_maps, component_registry)?;
    remove_if_exists(&data_dir.join(SHUTDOWN_FILE))?;
    remove_if_exists(&data_dir.join(SHUTDOWN_OWNER_FILE))?;
    let saved_clock = loaded_state.saved_clock;
    let entity_counters = loaded_state.entity_counters;
    let world_initialization = loaded_state.world_initialization;
    let entities = loaded_state.entities;
    let mut player_store = loaded_state.player_store;
    if let Some(definition) = planet_definition {
        let resolved_path = data_dir.join("world.planet.ron");
        if !resolved_path.exists() {
            minimium_platform::files::atomic_write_file(
                &data_dir.join("world.planet.ron.tmp"),
                &resolved_path,
                &definition,
            )?;
        }
    }
    if map_path.is_none() && world_definition_path.is_none() && existing_seed.is_none() {
        minimium_platform::files::atomic_write_file(
            &data_dir.join("world.seed.tmp"),
            &seed_path,
            format!("{seed}\n").as_bytes(),
        )?;
    }
    player_store.reserve_active(config.max_players);
    let next_player = player_store.next_player_id()?;
    let transport_endpoint = transport.endpoint();
    let game_address = transport_endpoint.game_address;
    let endpoint = ServerEndpoint {
        game_port: transport_endpoint.game_port,
        admin_port: transport_endpoint.admin_port,
        token: transport_endpoint.admin_token,
    };
    if endpoint.token.len() != 64 || !endpoint.token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(
            io::Error::new(io::ErrorKind::InvalidInput, "invalid inherited admin token").into(),
        );
    }
    ServerEndpoint::write(data_dir, &endpoint)?;
    let endpoint_guard = EndpointGuard {
        data_dir: PathBuf::from(data_dir),
        endpoint: endpoint.clone(),
        logger: server_logger,
    };
    let process_lifecycle = Box::new(FileProcessLifecycle {
        data_dir: PathBuf::from(data_dir),
        endpoint: endpoint.clone(),
        _endpoint_guard: endpoint_guard,
        _data_lock: data_lock,
    });
    let climate_config = ClimateConfiguration::load(config)?;
    let content = ContentServer::new(config, asset_io_logger)?;
    climate_config.commit()?;
    let climate = climate_config.scenario;
    let storage_services = storage_backend.open_services()?;
    Ok(PreparedRuntime {
        map,
        primary_terrain_material_profile,
        worlds: generated_worlds,
        player_store,
        entity_counters,
        world_initialization,
        entities,
        persistence: storage_services.persistence,
        excavation_journal: storage_services.excavation,
        excavation_restore: storage_services.excavation_restore,
        impact_journal: storage_services.impacts,
        impact_restore: storage_services.impact_restore,
        climate,
        content: Box::new(content),
        next_player,
        transport,
        game_address,
        endpoint: Some(endpoint),
        process_lifecycle: Some(process_lifecycle),
        environment_config,
        saved_clock,
        prepared_geography,
    })
}

pub(crate) fn preflight_persistent_formats(data_dir: &Path) -> io::Result<()> {
    minimium_storage::FileStorageBackend::preflight_path(data_dir)
}

fn read_optional_file(path: &Path, maximum: usize) -> io::Result<Option<Vec<u8>>> {
    match minimium_platform::files::read_bounded_file(path, maximum) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn parse_seed(bytes: Option<&[u8]>) -> io::Result<Option<u128>> {
    bytes
        .map(|bytes| {
            std::str::from_utf8(bytes)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid world.seed"))?
                .trim()
                .parse::<u128>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid world.seed"))
        })
        .transpose()
}

fn authored_manifest(id: u128, fingerprint: u64) -> minimium_world::world::WorldManifest {
    use minimium_world::world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, WorldContentRevision,
        WorldDefinitionId, WorldManifest, WorldSeed, WorldTopology,
    };
    let generation = GenerationSpec::new(
        WorldSeed::new(0),
        GenerationRevision::new(6).expect("current generator revision is nonzero"),
        1,
    )
    .expect("authored position quantum is nonzero");
    WorldManifest::new(
        WorldDefinitionId::new(id).expect("authored definition ID is nonzero"),
        WorldTopology::Dungeon,
        generation,
        WorldContentRevision::new(1).expect("authored content revision is nonzero"),
        GeneratorDigest::authored_map_fingerprint(fingerprint),
    )
}
