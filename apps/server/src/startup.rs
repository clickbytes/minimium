use crate::{
    admin::AdminServer,
    lifecycle::{remove_if_exists, EndpointGuard},
    storage::{PersistenceWorker, PlayerStore},
};
use minimium_core::world::WorldMap;
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
    bind_address: &str,
    game_port: u16,
    max_players: usize,
) -> Result<Startup, Box<dyn std::error::Error>> {
    let bind_addresses: Vec<_> = (bind_address, game_port).to_socket_addrs()?.collect();
    if bind_addresses.is_empty() {
        return Err("bind address resolved to no addresses".into());
    }

    let map = if let Some(path) = map_path {
        WorldMap::load_file(path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("could not load map {}: {error}", path.display()),
            )
        })?
    } else {
        WorldMap::parse(SAMPLE_MAP)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
    };

    let data_lock = ServerDataLock::acquire(data_dir)?;
    remove_if_exists(&data_dir.join(SHUTDOWN_FILE))?;
    remove_if_exists(&data_dir.join(SHUTDOWN_OWNER_FILE))?;
    let mut player_store = PlayerStore::load(data_dir, &map)?;
    player_store.active_players.reserve(max_players);
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
