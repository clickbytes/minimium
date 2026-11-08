use crate::{
    admin::AdminServer,
    composition::{WorldComposer, WorldCompositionRequest, WorldDefinition},
    config::ServerConfig,
    environment::{self, EnvironmentConfig},
    lifecycle::{remove_if_exists, EndpointGuard},
    storage::{PersistenceWorker, PlayerStore},
};
use minimium_environment::time::SavedClockState;
use minimium_platform::{
    admin_client::{SHUTDOWN_FILE, SHUTDOWN_OWNER_FILE},
    endpoint::{new_admin_token, ServerEndpoint},
    server_lock::ServerDataLock,
};
use minimium_world::world::{WorldInstanceId, WorldMap};
use std::{
    io,
    net::{TcpListener, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) struct Startup {
    pub(crate) map: WorldMap,
    pub(crate) worlds: Option<Vec<WorldDefinition>>,
    pub(crate) player_store: PlayerStore,
    pub(crate) entity_counters: Vec<(WorldInstanceId, u64)>,
    pub(crate) world_initialization: Vec<(WorldInstanceId, bool)>,
    pub(crate) entities: Vec<minimium_storage::PersistedEntity>,
    pub(crate) persistence: PersistenceWorker,
    pub(crate) next_player: u64,
    pub(crate) game_listener: TcpListener,
    pub(crate) game_address: std::net::SocketAddr,
    pub(crate) endpoint: ServerEndpoint,
    pub(crate) endpoint_guard: EndpointGuard,
    pub(crate) data_lock: ServerDataLock,
    pub(crate) admin: AdminServer,
    pub(crate) environment_config: EnvironmentConfig,
    pub(crate) saved_clock: Option<SavedClockState>,
    pub(crate) prepared_geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
}

pub(crate) fn initialize(
    config: &ServerConfig,
    world_composer: WorldComposer,
    component_registry: &minimium_world::entity::ComponentRegistry,
    storage_logger: minimium_diagnostics::DomainLogger,
    server_logger: minimium_diagnostics::DomainLogger,
) -> Result<Startup, Box<dyn std::error::Error>> {
    let data_dir = &config.data_dir;
    let requested_seed = config.seed;
    let map_path = config.map_path.as_deref();
    let world_definition_path = config.world_definition_path.as_deref();
    let planet_config_path = config.planet_config_path.as_deref();
    let bind_address = config.bind_address.as_str();
    let game_port = config.game_port;
    let max_players = config.max_players;
    let bind_addresses: Vec<_> = (bind_address, game_port).to_socket_addrs()?.collect();
    if bind_addresses.is_empty() {
        return Err("bind address resolved to no addresses".into());
    }

    let data_lock = ServerDataLock::acquire(data_dir)?;
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
    let existing_seed = match minimium_platform::files::read_bounded_file(&seed_path, 64) {
        Ok(bytes) => Some(
            std::str::from_utf8(&bytes)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid world.seed"))?
                .trim()
                .parse::<u128>()
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid world.seed"))?,
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let composition = world_composer(WorldCompositionRequest {
        data_dir: data_dir.to_path_buf(),
        requested_seed,
        existing_seed,
        map_path: map_path.map(Path::to_path_buf),
        world_definition_path: world_definition_path.map(Path::to_path_buf),
        planet_config_path: planet_config_path.map(Path::to_path_buf),
    })?;
    let seed = composition.seed;
    let map = composition.primary_map;
    let generated_worlds = composition.worlds;
    let planet_definition = composition.planet_definition;
    let prepared_geography = composition.prepared_geography;

    remove_if_exists(&data_dir.join(SHUTDOWN_FILE))?;
    remove_if_exists(&data_dir.join(SHUTDOWN_OWNER_FILE))?;
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
    let loaded_state = PlayerStore::load(data_dir, &world_maps, component_registry)?;
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
    player_store.reserve_active(max_players);
    let persistence = PersistenceWorker::spawn(data_dir.to_path_buf(), storage_logger.clone())?;
    let next_player = player_store.next_player_id()?;
    let game_listener = TcpListener::bind(bind_addresses.as_slice())?;
    let game_address = game_listener.local_addr()?;
    let admin_listener = TcpListener::bind(("127.0.0.1", 0))?;
    game_listener.set_nonblocking(true)?;
    admin_listener.set_nonblocking(true)?;
    let endpoint = ServerEndpoint {
        game_port: game_address.port(),
        admin_port: admin_listener.local_addr()?.port(),
        token: match config.admin_token.as_deref() {
            Some(token)
                if token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
            {
                token.to_owned()
            }
            Some(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "invalid inherited admin token",
                )
                .into())
            }
            None => new_admin_token()?,
        },
    };
    ServerEndpoint::write(data_dir, &endpoint)?;
    let endpoint_guard = EndpointGuard {
        data_dir: PathBuf::from(data_dir),
        endpoint: endpoint.clone(),
        logger: server_logger,
    };
    let admin = AdminServer::new(admin_listener, endpoint.token.clone())?;

    Ok(Startup {
        map,
        worlds: generated_worlds,
        player_store,
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
    })
}

fn authored_manifest(id: u128, fingerprint: u64) -> minimium_world::world::WorldManifest {
    use minimium_world::world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, WorldContentRevision,
        WorldDefinitionId, WorldManifest, WorldSeed, WorldTopology,
    };
    let generation = GenerationSpec::new(
        WorldSeed::new(0),
        GenerationRevision::new(5).expect("current generator revision is nonzero"),
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
