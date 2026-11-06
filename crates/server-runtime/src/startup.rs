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
use minimium_world::world::{
    WorldContentRevision, WorldDefinitionId, WorldInstanceId, WorldManifest, WorldMap,
    WorldTopology,
};
use std::{
    env, io,
    net::{TcpListener, ToSocketAddrs},
    path::{Path, PathBuf},
    sync::Arc,
};

pub(crate) struct Startup {
    pub(crate) map: WorldMap,
    pub(crate) worlds: Option<Vec<WorldDefinition>>,
    pub(crate) player_store: PlayerStore,
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
    // Recover if the migrated snapshot was committed before world.seed.
    let snapshot_seed =
        if existing_seed.is_none() && map_path.is_none() && world_definition_path.is_none() {
            match minimium_platform::files::read_bounded_file(
                &data_dir.join("world.snapshot"),
                16 * 1024 * 1024,
            ) {
                Ok(snapshot) => {
                    let source = std::str::from_utf8(&snapshot).map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "snapshot is not UTF-8")
                    })?;
                    let fields: Vec<_> = source.lines().next().unwrap_or("").split('\t').collect();
                    let recognized_planet = fields.first() == Some(&"MINIMIUM_WORLD")
                        && matches!(fields.get(1).copied(), Some("6" | "7" | "8"))
                        && fields.get(3) == Some(&"1")
                        && fields.get(4) == Some(&"0")
                        && fields
                            .get(6)
                            .and_then(|value| value.parse::<u32>().ok())
                            .is_some_and(|revision| revision >= 2);
                    if recognized_planet {
                        fields.get(5).and_then(|value| value.parse::<u128>().ok())
                    } else {
                        None
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.into()),
            }
        } else {
            None
        };
    let migrate_demo = existing_seed.is_none()
        && snapshot_seed.is_none()
        && data_dir.join("world.snapshot").exists()
        && map_path.is_none()
        && world_definition_path.is_none();
    let random_seed = if existing_seed.is_none()
        && snapshot_seed.is_none()
        && requested_seed.is_none()
        && !migrate_demo
        && !data_dir.join("world.planet.ron").exists()
    {
        let random_token = new_admin_token()?;
        u128::from_str_radix(&random_token[..32], 16)
            .expect("an admin token contains hexadecimal digits")
    } else {
        0
    };
    let composition = world_composer(WorldCompositionRequest {
        data_dir: data_dir.to_path_buf(),
        requested_seed,
        existing_seed,
        snapshot_seed,
        map_path: map_path.map(Path::to_path_buf),
        world_definition_path: world_definition_path.map(Path::to_path_buf),
        planet_config_path: planet_config_path.map(Path::to_path_buf),
        legacy_snapshot: migrate_demo,
        random_seed,
    })?;
    let seed = composition.seed;
    let map = composition.primary_map;
    let generated_worlds = composition.worlds;
    let migration_worlds = composition.migration_worlds;
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
        .unwrap_or_else(|| vec![(primary, map.clone(), legacy_manifest(1, map.fingerprint()))]);
    if migrate_demo {
        migrate_legacy_demo(
            data_dir,
            generated_worlds.as_ref().expect("generated worlds exist"),
            migration_worlds.as_ref().expect("migration worlds exist"),
        )?;
    }
    let loaded_state = PlayerStore::load(data_dir, &world_maps)?;
    if let Some(version) = loaded_state.source_version.filter(|version| *version < 8) {
        println!("legacy world snapshot v{version}: initializing world clock as real_time");
    }
    let saved_clock = loaded_state.saved_clock;
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
    let persistence = PersistenceWorker::spawn(data_dir.to_path_buf())?;
    let next_player = player_store.next_player_id()?;
    let game_listener = TcpListener::bind(bind_addresses.as_slice())?;
    let game_address = game_listener.local_addr()?;
    let admin_listener = TcpListener::bind(("127.0.0.1", 0))?;
    game_listener.set_nonblocking(true)?;
    admin_listener.set_nonblocking(true)?;
    let endpoint = ServerEndpoint {
        game_port: game_address.port(),
        admin_port: admin_listener.local_addr()?.port(),
        token: match env::var("MINIMIUM_ADMIN_TOKEN") {
            Ok(token)
                if token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
            {
                token
            }
            Ok(_) => return Err("invalid inherited admin token".into()),
            Err(env::VarError::NotPresent) => new_admin_token()?,
            Err(_) => return Err("invalid inherited admin token".into()),
        },
    };
    ServerEndpoint::write(data_dir, &endpoint)?;
    let endpoint_guard = EndpointGuard {
        data_dir: PathBuf::from(data_dir),
        endpoint: endpoint.clone(),
    };
    let admin = AdminServer::new(admin_listener, endpoint.token.clone())?;

    Ok(Startup {
        map,
        worlds: generated_worlds,
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
        prepared_geography,
    })
}

fn migrate_legacy_demo(
    data_dir: &Path,
    generated_worlds: &[WorldDefinition],
    migration_worlds: &[WorldDefinition],
) -> io::Result<()> {
    let old_maps: Vec<_> = migration_worlds
        .iter()
        .map(|world| (world.instance, world.map.clone(), world.manifest))
        .collect();
    let mut players = PlayerStore::load(data_dir, &old_maps)?.player_store;
    players.migrate_legacy_planet_positions();
    let mut registry =
        crate::world_registry::WorldRegistry::try_from_world_definitions(generated_worlds.to_vec())
            .map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("generated world registry is invalid: {error:?}"),
                )
            })?;
    registry
        .restore_sector_states(players.sector_states())
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("legacy sector state: {error:?}"),
            )
        })?;
    let saved_clock = environment::initial_saved_clock_state().map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("environment clock: {error}"),
        )
    })?;
    let input = registry.snapshot_for_save(registry.primary().map.fingerprint())?;
    let snapshot = players.save_snapshot(input, saved_clock)?;
    let source = data_dir.join("world.snapshot");
    let backup = data_dir.join("world.snapshot.legacy-v1");
    if !backup.exists() {
        std::fs::copy(&source, &backup)?;
    }
    snapshot.write(data_dir)
}

fn legacy_manifest(id: u128, fingerprint: u64) -> WorldManifest {
    use minimium_world::world::{GenerationRevision, GenerationSpec, GeneratorDigest, WorldSeed};
    let generation = GenerationSpec::new(
        WorldSeed::new(0),
        GenerationRevision::new(1).expect("legacy generator revision is nonzero"),
        1,
    )
    .expect("legacy position quantum is nonzero");
    WorldManifest::new(
        WorldDefinitionId::new(id).expect("legacy definition ID is nonzero"),
        WorldTopology::Dungeon,
        generation,
        WorldContentRevision::new(1).expect("legacy content revision is nonzero"),
        GeneratorDigest::legacy_map_fingerprint(fingerprint),
    )
}
