use crate::{
    admin::AdminServer,
    config::ServerConfig,
    environment::{self, EnvironmentConfig},
    lifecycle::{remove_if_exists, EndpointGuard},
    planet_config::{self, PlanetMode, ResolvedPlanetConfig},
    storage::{PersistenceWorker, PlayerStore},
};
use minimium_core::world::{
    WorldContentRevision, WorldDefinitionId, WorldInstanceId, WorldManifest, WorldMap,
    WorldTopology,
};
use minimium_platform::{
    admin_client::{SHUTDOWN_FILE, SHUTDOWN_OWNER_FILE},
    endpoint::{new_admin_token, ServerEndpoint},
    server_lock::ServerDataLock,
};
use std::{
    env, io,
    net::{TcpListener, ToSocketAddrs},
    path::{Path, PathBuf},
};

const SAMPLE_MAP: &str = include_str!("../../../games/sample/maps/demo.map");

pub(crate) struct Startup {
    pub(crate) map: WorldMap,
    pub(crate) generated_worlds: Option<Vec<minimium_procgen::GeneratedWorld>>,
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
}

pub(crate) fn initialize(config: &ServerConfig) -> Result<Startup, Box<dyn std::error::Error>> {
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
    let resolved_path = data_dir.join("world.planet.ron");
    let saved_planet = match std::fs::read_to_string(&resolved_path) {
        Ok(source) => Some(
            ron::from_str::<ResolvedPlanetConfig>(&source).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("saved planet definition: {error}"),
                )
            })?,
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let (Some(file_seed), Some(planet)) = (existing_seed, &saved_planet) {
        if file_seed != planet.seed {
            return Err("world.seed differs from the saved planet definition".into());
        }
    }
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
                    if fields.first() == Some(&"MINIMIUM_WORLD")
                        && fields.get(1) == Some(&"6")
                        && fields.get(3) == Some(&"1")
                        && fields.get(4) == Some(&"0")
                        && fields.get(6) == Some(&"2")
                    {
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
    if let (Some(expected), Some(requested)) = (
        existing_seed
            .or(snapshot_seed)
            .or(saved_planet.as_ref().map(|planet| planet.seed)),
        requested_seed,
    ) {
        if expected != requested {
            return Err("--seed differs from the seed stored in this data directory".into());
        }
    }
    let migrate_demo = existing_seed.is_none()
        && snapshot_seed.is_none()
        && data_dir.join("world.snapshot").exists()
        && map_path.is_none()
        && world_definition_path.is_none();
    if migrate_demo && requested_seed.is_some_and(|requested| requested != 0x0030_0000_0000_0001) {
        return Err("existing demo snapshot requires its original seed during migration".into());
    }
    let seed = match existing_seed
        .or(snapshot_seed)
        .or(saved_planet.as_ref().map(|planet| planet.seed))
        .or(requested_seed)
    {
        Some(seed) => seed,
        None if migrate_demo => 0x0030_0000_0000_0001,
        None => {
            let token = new_admin_token()?;
            u128::from_str_radix(&token[..32], 16)
                .expect("an admin token contains hexadecimal digits")
        }
    };
    let legacy_generated = (existing_seed.is_some() || data_dir.join("world.snapshot").exists())
        && saved_planet.is_none();
    if legacy_generated && planet_config_path.is_some() {
        return Err("existing generated world has a legacy definition; use a new data directory for a planet preset".into());
    }
    let mut planet = if map_path.is_none() && world_definition_path.is_none() && !legacy_generated {
        let requested = planet_config_path
            .map(|path| planet_config::load(path, seed))
            .transpose()?;
        let resolved = match (saved_planet, requested) {
            (Some(saved), Some(mut requested)) => {
                requested.package_digest = saved.package_digest;
                requested.generator_revision = saved.generator_revision;
                if saved != requested {
                    return Err("planet preset differs from the immutable definition in this data directory".into());
                }
                saved
            }
            (Some(saved), _) => saved,
            (None, Some(requested)) => requested,
            (None, None) => ResolvedPlanetConfig::procedural(seed),
        };
        resolved.validate()?;
        Some(resolved)
    } else {
        None
    };

    let (map, generated_worlds) = if let Some(path) = world_definition_path {
        let definition = minimium_assets::load_world_definition(path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "could not load world definition {}: {error}",
                    path.display()
                ),
            )
        })?;
        let map = minimium_procgen::materialize_dungeon_map(definition.dungeon())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let instance = WorldInstanceId::new(4).expect("manual world instance ID is nonzero");
        let generated = minimium_procgen::GeneratedWorld {
            instance,
            manifest: definition.manifest(),
            map: map.clone(),
            portal: None,
        };
        let legacy_map = minimium_assets::parse_map(SAMPLE_MAP)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        (map, Some(vec![generated, legacy_world(legacy_map)]))
    } else if let Some(path) = map_path {
        let map = minimium_assets::load_map(path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("could not load map {}: {error}", path.display()),
            )
        })?;
        (map, None)
    } else {
        let generated = if let Some(planet) = &mut planet {
            if planet.mode == PlanetMode::Earth {
                let package = planet
                    .earth_package
                    .clone()
                    .or_else(|| env::var_os("MINIMIUM_EARTH_PACKAGE").map(PathBuf::from))
                    .unwrap_or_else(|| data_dir.join("gebco-2026"));
                if !package.join("index.ron").is_file() {
                    return Err(format!(
                        "GEBCO 2026 ice-surface package missing at {}; run import-gebco.ps1 -GridPath GEBCO_2026.nc -DataDir {}",
                        package.display(), data_dir.display()
                    ).into());
                }
                let digest = minimium_procgen::install_earth_package(&package, None)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                if planet.package_digest != [0; 32] && planet.package_digest != digest {
                    return Err("GEBCO package differs from the immutable world definition".into());
                }
                planet.package_digest = digest;
                env::set_var("MINIMIUM_EARTH_PACKAGE", &package);
            }
            minimium_procgen::generate_worlds_with_planet_revision(
                seed,
                planet.definition(),
                planet.generator_revision,
            )
        } else {
            minimium_procgen::generate_worlds(seed)
        };
        let mut worlds = generated
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .into_iter()
            .collect::<Vec<_>>();
        let legacy_map = minimium_assets::parse_map(SAMPLE_MAP)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        worlds.push(legacy_world(legacy_map));
        (worlds[0].map.clone(), Some(worlds))
    };

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
        )?;
    }
    let mut player_store = PlayerStore::load(data_dir, &world_maps)?;
    if let Some(planet) = &planet {
        if !resolved_path.exists() {
            let source = ron::to_string(planet)?;
            minimium_platform::files::atomic_write_file(
                &data_dir.join("world.planet.ron.tmp"),
                &resolved_path,
                source.as_bytes(),
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
    })
}

fn migrate_legacy_demo(
    data_dir: &Path,
    generated_worlds: &[minimium_procgen::GeneratedWorld],
) -> io::Result<()> {
    let old = minimium_procgen::generate_legacy_demo_worlds()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let legacy_map = minimium_assets::parse_map(SAMPLE_MAP)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut old_maps: Vec<_> = old
        .into_iter()
        .map(|world| (world.instance, world.map, world.manifest))
        .collect();
    old_maps.push((
        WorldInstanceId::new(3).expect("legacy instance is nonzero"),
        legacy_map.clone(),
        legacy_manifest(3, legacy_map.fingerprint()),
    ));
    let mut players = PlayerStore::load(data_dir, &old_maps)?;
    players.migrate_legacy_planet_positions();
    let mut registry =
        crate::world_registry::WorldRegistry::from_generated_worlds(generated_worlds.to_vec());
    registry
        .restore_sector_states(players.sector_states())
        .map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("legacy sector state: {error:?}"),
            )
        })?;
    let snapshot = players.save_snapshot(&registry, registry.primary().map.fingerprint())?;
    let source = data_dir.join("world.snapshot");
    let backup = data_dir.join("world.snapshot.legacy-v1");
    if !backup.exists() {
        std::fs::copy(&source, &backup)?;
    }
    snapshot.write(data_dir)
}

fn legacy_world(map: WorldMap) -> minimium_procgen::GeneratedWorld {
    use minimium_core::world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, WorldContentRevision,
        WorldDefinitionId, WorldInstanceId, WorldManifest, WorldSeed, WorldTopology,
    };
    let instance = WorldInstanceId::new(3).expect("legacy instance ID is nonzero");
    let generation = GenerationSpec::new(
        WorldSeed::new(0),
        GenerationRevision::new(1).expect("legacy generator revision is nonzero"),
        1,
    )
    .expect("legacy position quantum is nonzero");
    let manifest = WorldManifest::new(
        WorldDefinitionId::new(3).expect("legacy definition ID is nonzero"),
        WorldTopology::Dungeon,
        generation,
        WorldContentRevision::new(1).expect("legacy content revision is nonzero"),
        GeneratorDigest::legacy_map_fingerprint(map.fingerprint()),
    );
    minimium_procgen::GeneratedWorld {
        instance,
        manifest,
        map,
        portal: None,
    }
}

fn legacy_manifest(id: u128, fingerprint: u64) -> WorldManifest {
    use minimium_core::world::{GenerationRevision, GenerationSpec, GeneratorDigest, WorldSeed};
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
