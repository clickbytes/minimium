use crate::{
    admin::AdminServer,
    lifecycle::{remove_if_exists, EndpointGuard},
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
}

pub(crate) fn initialize(
    data_dir: &Path,
    map_path: Option<&Path>,
    world_definition_path: Option<&Path>,
    bind_address: &str,
    game_port: u16,
    max_players: usize,
) -> Result<Startup, Box<dyn std::error::Error>> {
    let bind_addresses: Vec<_> = (bind_address, game_port).to_socket_addrs()?.collect();
    if bind_addresses.is_empty() {
        return Err("bind address resolved to no addresses".into());
    }

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
        let mut worlds = minimium_procgen::generate_demo_worlds()
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .into_iter()
            .collect::<Vec<_>>();
        let legacy_map = minimium_assets::parse_map(SAMPLE_MAP)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        worlds.push(legacy_world(legacy_map));
        (worlds[0].map.clone(), Some(worlds))
    };

    let data_lock = ServerDataLock::acquire(data_dir)?;
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
    let mut player_store = PlayerStore::load(data_dir, &world_maps)?;
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
    })
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
