mod admin;
mod commands;
mod metrics;
mod network;
mod terminal;
use metrics::RuntimeMetrics;
use minimium_core::server_lock::ServerDataLock;
use minimium_telemetry::allocations::{self, Phase};
#[cfg(feature = "allocation-metrics")]
#[global_allocator]
static ALLOCATOR: allocations::CountingAllocator = allocations::CountingAllocator;
use minimium_core::files::atomic_replace_file;
use minimium_core::{
    commands::ServerCommand,
    endpoint::{new_admin_token, ServerEndpoint},
    executor::Executor,
    identity::PlayerProfileId,
    world::{CharacterInput, Player, World, WorldMap, PLAYER_MAX_HEALTH},
};
use minimium_protocol::{
    decode_client, encode_server, encode_server_into, ClientMessage, RemotePlayer, RemotePlayerIds,
    RemotePlayers, ServerMessage, ERROR_BUSY, ERROR_RATE_LIMIT, ERROR_SERVER_FULL, HEADER_BYTES,
    MAX_FRAME_BYTES, MAX_VISIBLE_PLAYERS,
};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    env,
    fs::{self, File},
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

const TICK: Duration = Duration::from_millis(50);
const MAX_PLAYERS: usize = 512;
const MAX_PENDING_HANDSHAKES: usize = 64;
const MAX_ACCEPTS_PER_POLL: usize = MAX_PENDING_HANDSHAKES;
const MAX_READ_BYTES_PER_CLIENT_PER_POLL: usize = 1024;
const SAMPLE_MAP: &str = include_str!("../../../games/sample/maps/demo.map");
const MAX_MESSAGES_PER_CLIENT_PER_POLL: usize = 32;
const MAX_PENDING_WORLD_INPUTS: usize = 4096;
const MAX_QUEUED_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_QUEUED_OUTPUT_FRAMES: usize = 256;
const CLIENT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const CLIENT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const SHOT_COOLDOWN: Duration = Duration::from_millis(250);
const CHAT_COOLDOWN: Duration = Duration::from_secs(1);
const GLOBAL_CHAT_COOLDOWN: Duration = Duration::from_millis(50);
const MAX_SERVER_FRAME_BYTES: usize =
    HEADER_BYTES + 36 + MAX_VISIBLE_PLAYERS * (45 + std::mem::size_of::<u64>());
const MAX_STORED_PLAYERS: usize = 100_000;
const MAX_WORLD_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SAVED_COORDINATE: f32 = 1_000_000.0;
const PLAYER_STORE_SHARDS: usize = 256;
const DEFAULT_AUTOSAVE_SECONDS: u64 = 300;

struct Client {
    player_id: Option<u64>,
    profile_id: Option<PlayerProfileId>,
    stream: TcpStream,
    input: Vec<u8>,
    output: Vec<u8>,
    output_offset: usize,
    output_frames: VecDeque<(usize, Instant)>,
    bytes_in: u64,
    bytes_out: u64,
    frames_in: u64,
    frames_out: u64,
    encoding_us: u64,
    replicated_players: RemotePlayers,
    replication_initialized: bool,
    connected_at: Instant,
    last_message_at: Instant,
    last_fire_at: Option<Instant>,
    last_chat_at: Option<Instant>,
    pending_health_update: Option<u16>,
}

enum WorldInput {
    Join(u64, String, [f32; 3], u16),
    Leave(u64),
    Move(u64, CharacterInput),
    Fire(u64, [f32; 3]),
    Chat(u64, String),
    Rename(u64, String),
}

#[derive(Clone, Copy)]
struct StoredPlayer {
    id: u64,
    name: StoredName,
    position: [f32; 3],
    health: u16,
}

#[derive(Clone, Copy)]
struct StoredName {
    bytes: [u8; 24],
    length: u8,
}

impl StoredName {
    fn new(value: &str) -> Option<Self> {
        if !valid_player_name(value) {
            return None;
        }
        let mut bytes = [0; 24];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Some(Self {
            bytes,
            length: value.len() as u8,
        })
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.length)]).unwrap_or("")
    }
}

struct PlayerStore {
    players: Vec<Arc<HashMap<PlayerProfileId, StoredPlayer>>>,
    profile_by_player: HashMap<u64, PlayerProfileId>,
    active_players: HashSet<u64>,
}

struct SaveSnapshot {
    map_fingerprint: u64,
    player_shards: Vec<Arc<HashMap<PlayerProfileId, StoredPlayer>>>,
}

enum PersistenceRequest {
    Save(SaveSnapshot),
    Shutdown(Option<SaveSnapshot>, mpsc::SyncSender<io::Result<()>>),
    Stop,
}

struct PersistenceWorker {
    sender: mpsc::SyncSender<PersistenceRequest>,
    completions: mpsc::Receiver<io::Result<()>>,
    thread: Option<thread::JoinHandle<()>>,
    pending_saves: usize,
    last_save: String,
}

impl Default for PlayerStore {
    fn default() -> Self {
        Self {
            players: (0..PLAYER_STORE_SHARDS)
                .map(|_| Arc::new(HashMap::new()))
                .collect(),
            profile_by_player: HashMap::new(),
            active_players: HashSet::new(),
        }
    }
}

fn player_store_shard(profile_id: PlayerProfileId) -> usize {
    usize::from(profile_id.as_bytes()[0])
}

impl PlayerStore {
    fn load(data_dir: &std::path::Path, map: &WorldMap) -> io::Result<Self> {
        let path = data_dir.join("world.snapshot");
        let mut source = String::new();
        match File::open(path) {
            Ok(file) => {
                file.take((MAX_WORLD_SNAPSHOT_BYTES + 1) as u64)
                    .read_to_string(&mut source)?;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(error),
        }
        if source.len() > MAX_WORLD_SNAPSHOT_BYTES {
            return Err(invalid_world_snapshot("snapshot exceeds size limit"));
        }

        let mut lines = source.lines();
        let header = lines
            .next()
            .ok_or_else(|| invalid_world_snapshot("snapshot is empty"))?;
        let mut header_fields = header.split('\t');
        let expected_fingerprint = format!("{:016x}", map.fingerprint());
        let signature = header_fields.next();
        let version = header_fields.next();
        let fingerprint = header_fields.next();
        if signature != Some("MINIMIUM_WORLD") || header_fields.next().is_some() {
            return Err(invalid_world_snapshot("invalid snapshot header"));
        }
        if version == Some("1") {
            return Err(invalid_world_snapshot(
                "snapshot version 1 has no client profile IDs; back up or remove the old snapshot before starting",
            ));
        }
        if version != Some("2") && version != Some("3") {
            return Err(invalid_world_snapshot("unsupported snapshot version"));
        }
        if fingerprint != Some(expected_fingerprint.as_str()) {
            return Err(invalid_world_snapshot(
                "snapshot map fingerprint does not match",
            ));
        }

        let mut store = Self::default();
        for line in lines {
            if store.profile_by_player.len() >= MAX_STORED_PLAYERS {
                return Err(invalid_world_snapshot("snapshot has too many players"));
            }
            let mut fields = line.split('\t');
            let id = parse_saved_id(fields.next())?;
            let profile_id = PlayerProfileId::from_hex(
                fields
                    .next()
                    .ok_or_else(|| invalid_world_snapshot("missing player profile id"))?,
            )
            .map_err(|_| invalid_world_snapshot("invalid player profile id"))?;
            let name = fields
                .next()
                .ok_or_else(|| invalid_world_snapshot("missing player name"))?;
            let name = StoredName::new(name)
                .ok_or_else(|| invalid_world_snapshot("invalid saved player name"))?;
            let position = [
                parse_saved_coordinate(fields.next())?,
                parse_saved_coordinate(fields.next())?,
                parse_saved_coordinate(fields.next())?,
            ];
            let health = if version == Some("3") {
                parse_saved_health(fields.next())?
            } else {
                PLAYER_MAX_HEALTH
            };
            if fields.next().is_some() || !map.can_place_player(position) {
                return Err(invalid_world_snapshot(
                    "saved player position overlaps map geometry",
                ));
            }
            let shard = player_store_shard(profile_id);
            let duplicate_profile = Arc::make_mut(&mut store.players[shard])
                .insert(
                    profile_id,
                    StoredPlayer {
                        id,
                        name,
                        position,
                        health,
                    },
                )
                .is_some();
            let duplicate_player = store.profile_by_player.insert(id, profile_id).is_some();
            if duplicate_profile || duplicate_player {
                return Err(invalid_world_snapshot("duplicate saved player identity"));
            }
        }
        Ok(store)
    }

    fn next_player_id(&self) -> io::Result<u64> {
        self.players
            .iter()
            .flat_map(|shard| shard.values())
            .map(|player| player.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| invalid_world_snapshot("player id space is exhausted"))
    }

    fn claim(
        &mut self,
        profile_id: PlayerProfileId,
        next_id: &mut u64,
        spawn: [f32; 3],
    ) -> io::Result<StoredPlayer> {
        let shard = player_store_shard(profile_id);
        if let Some(existing) = self.players[shard].get(&profile_id) {
            if self.active_players.contains(&existing.id) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    "player profile is already connected",
                ));
            }
        } else {
            if self.profile_by_player.len() >= MAX_STORED_PLAYERS {
                return Err(io::Error::other(
                    "server has reached the saved player limit",
                ));
            }
            let id = *next_id;
            *next_id = id
                .checked_add(1)
                .ok_or_else(|| io::Error::other("player id space is exhausted"))?;
            let default_name = format!("Player{id}");
            let name = StoredName::new(&default_name)
                .or_else(|| {
                    let fallback_name = format!("P{id}");
                    StoredName::new(&fallback_name)
                })
                .ok_or_else(|| io::Error::other("could not create default player name"))?;
            let player = StoredPlayer {
                id,
                name,
                position: spawn,
                health: PLAYER_MAX_HEALTH,
            };
            Arc::make_mut(&mut self.players[shard]).insert(profile_id, player);
            self.profile_by_player.insert(id, profile_id);
        }

        let player = self.players[shard]
            .get(&profile_id)
            .copied()
            .ok_or_else(|| io::Error::other("player record disappeared"))?;
        if !self.active_players.insert(player.id) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "player profile is already connected",
            ));
        }
        Ok(player)
    }

    fn release(&mut self, player_id: u64) {
        self.active_players.remove(&player_id);
    }

    fn update_from_player(&mut self, player: &Player) -> io::Result<()> {
        let profile_id = self
            .profile_by_player
            .get(&player.id)
            .copied()
            .ok_or_else(|| invalid_world_snapshot("active player has no profile id"))?;
        let shard = player_store_shard(profile_id);
        let stored = Arc::make_mut(&mut self.players[shard])
            .get_mut(&profile_id)
            .ok_or_else(|| invalid_world_snapshot("active player record is missing"))?;
        stored.name = StoredName::new(player.name.as_ref())
            .ok_or_else(|| invalid_world_snapshot("active player has an invalid name"))?;
        stored.position = player.position;
        stored.health = player.health;
        Ok(())
    }

    fn name_for_player(&self, player_id: u64) -> Option<&str> {
        let profile_id = self.profile_by_player.get(&player_id)?;
        let shard = player_store_shard(*profile_id);
        self.players[shard]
            .get(profile_id)
            .map(|player| player.name.as_str())
    }

    fn rename(&mut self, player_id: u64, name: String) {
        if let Some(profile_id) = self.profile_by_player.get(&player_id).copied() {
            let shard = player_store_shard(profile_id);
            if let Some(player) = Arc::make_mut(&mut self.players[shard]).get_mut(&profile_id) {
                if let Some(name) = StoredName::new(&name) {
                    player.name = name;
                }
            }
        }
    }

    fn save_snapshot(&mut self, world: &World, map_fingerprint: u64) -> io::Result<SaveSnapshot> {
        for player in world.snapshot() {
            self.update_from_player(&player)?;
        }

        let player_shards = self.players.iter().map(Arc::clone).collect();
        Ok(SaveSnapshot {
            map_fingerprint,
            player_shards,
        })
    }
}

impl SaveSnapshot {
    fn write(self, data_dir: &std::path::Path) -> io::Result<()> {
        let mut players: Vec<_> = self
            .player_shards
            .iter()
            .flat_map(|shard| {
                shard
                    .iter()
                    .map(|(profile_id, player)| (player.id, *profile_id, *player))
            })
            .collect();
        players.sort_unstable_by_key(|(id, _, _)| *id);
        let path = data_dir.join("world.snapshot");
        let temporary_path = data_dir.join("world.snapshot.tmp");
        let result = (|| {
            let mut file = File::create(&temporary_path)?;
            writeln!(file, "MINIMIUM_WORLD\t3\t{:016x}", self.map_fingerprint)?;
            for (_, profile_id, player) in players {
                writeln!(
                    file,
                    "{}\t{}\t{}\t{:.6}\t{:.6}\t{:.6}\t{}",
                    player.id,
                    profile_id.to_hex(),
                    player.name.as_str(),
                    player.position[0],
                    player.position[1],
                    player.position[2],
                    player.health
                )?;
            }
            file.sync_all()?;
            atomic_replace_file(&temporary_path, &path)?;
            if let Ok(directory) = File::open(data_dir) {
                let _ = directory.sync_all();
            }
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary_path);
        }
        result
    }
}

impl PersistenceWorker {
    fn spawn(data_dir: PathBuf) -> io::Result<Self> {
        let (sender, requests) = mpsc::sync_channel::<PersistenceRequest>(1);
        let (completion_sender, completions) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("minimium-storage".into())
            .spawn(move || {
                let _storage_phase = allocations::enter(Phase::Storage);
                while let Ok(request) = requests.recv() {
                    match request {
                        PersistenceRequest::Save(snapshot) => {
                            let _ = completion_sender.send(snapshot.write(&data_dir));
                        }
                        PersistenceRequest::Shutdown(snapshot, reply) => {
                            let result =
                                snapshot.map_or(Ok(()), |snapshot| snapshot.write(&data_dir));
                            let _ = reply.send(result);
                            break;
                        }
                        PersistenceRequest::Stop => break,
                    }
                }
            })?;
        Ok(Self {
            sender,
            completions,
            thread: Some(worker),
            pending_saves: 0,
            last_save: String::from("never"),
        })
    }

    fn queue_save(&mut self, snapshot: SaveSnapshot) -> io::Result<()> {
        match self.sender.try_send(PersistenceRequest::Save(snapshot)) {
            Ok(()) => {
                self.pending_saves = self.pending_saves.saturating_add(1);
                self.last_save = String::from("pending");
                Ok(())
            }
            Err(mpsc::TrySendError::Full(_)) => Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "a world save is already queued",
            )),
            Err(mpsc::TrySendError::Disconnected(_)) => Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "world storage worker stopped",
            )),
        }
    }

    fn queue_world_save(
        &mut self,
        player_store: &mut PlayerStore,
        world: &World,
        map_fingerprint: u64,
    ) -> io::Result<()> {
        if self.pending_saves > 0 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "a world save is already pending",
            ));
        }
        let snapshot = player_store.save_snapshot(world, map_fingerprint)?;
        self.queue_save(snapshot)
    }

    fn poll_completions(&mut self) {
        while let Ok(result) = self.completions.try_recv() {
            self.pending_saves = self.pending_saves.saturating_sub(1);
            match result {
                Ok(()) => {
                    self.last_save = String::from("ok");
                    println!("world save completed");
                }
                Err(error) => {
                    let message = compact_error(&error);
                    self.last_save = format!("error_{message}");
                    eprintln!("world save failed: {error}");
                }
            }
        }
    }

    fn summary(&self) -> String {
        format!(
            "save_pending={} save_last={}",
            self.pending_saves, self.last_save
        )
    }

    fn shutdown(&mut self, snapshot: Option<SaveSnapshot>) -> io::Result<()> {
        let (reply_sender, reply_receiver) = mpsc::sync_channel(1);
        let send_result = self
            .sender
            .send(PersistenceRequest::Shutdown(snapshot, reply_sender))
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "world storage worker stopped"));
        let save_result = if send_result.is_ok() {
            match reply_receiver.recv() {
                Ok(result) => result,
                Err(_) => Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "world storage worker stopped before shutdown completed",
                )),
            }
        } else {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "world storage worker stopped before shutdown was queued",
            ))
        };
        let join_result = self.join_worker();
        self.poll_completions();
        send_result?;
        join_result?;
        save_result
    }

    fn join_worker(&mut self) -> io::Result<()> {
        if let Some(worker) = self.thread.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("world storage worker panicked"))?;
        }
        Ok(())
    }
}

impl Drop for PersistenceWorker {
    fn drop(&mut self) {
        if self.thread.is_some() {
            let _ = self.sender.send(PersistenceRequest::Stop);
            let _ = self.join_worker();
            self.poll_completions();
        }
    }
}

fn compact_error(error: &io::Error) -> String {
    error
        .to_string()
        .chars()
        .take(120)
        .map(|character| {
            if character.is_ascii_whitespace() {
                '_'
            } else {
                character
            }
        })
        .collect()
}

fn remove_if_exists(path: &std::path::Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn write_shutdown_status(
    data_dir: &std::path::Path,
    endpoint: &ServerEndpoint,
    result: &io::Result<()>,
) -> io::Result<()> {
    let admin_port = endpoint.admin_port;
    let path = data_dir.join("server.shutdown");
    let temporary_path = data_dir.join("server.shutdown.tmp");
    let write_result = (|| {
        let mut file = File::create(&temporary_path)?;
        match result {
            Ok(()) => writeln!(file, "{admin_port}\tOK")?,
            Err(error) => writeln!(file, "{admin_port}\tERR\t{}", compact_error(error))?,
        }
        file.sync_all()?;
        // Keep the legacy status body; a private sidecar binds it to the full instance token.
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
        atomic_replace_file(
            &data_dir.join("server.shutdown.owner.tmp"),
            &data_dir.join("server.shutdown.owner"),
        )?;
        atomic_replace_file(&temporary_path, &path)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(temporary_path);
    }
    write_result
}

fn valid_player_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 24
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn parse_saved_id(value: Option<&str>) -> io::Result<u64> {
    value
        .ok_or_else(|| invalid_world_snapshot("missing player id"))?
        .parse::<u64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| invalid_world_snapshot("invalid player id"))
}

fn parse_saved_coordinate(value: Option<&str>) -> io::Result<f32> {
    let coordinate = value
        .ok_or_else(|| invalid_world_snapshot("missing player coordinate"))?
        .parse::<f32>()
        .map_err(|_| invalid_world_snapshot("invalid player coordinate"))?;
    if !coordinate.is_finite() || coordinate.abs() > MAX_SAVED_COORDINATE {
        return Err(invalid_world_snapshot("player coordinate is out of range"));
    }
    Ok(coordinate)
}

fn parse_saved_health(value: Option<&str>) -> io::Result<u16> {
    let health = value
        .ok_or_else(|| invalid_world_snapshot("missing player health"))?
        .parse::<u16>()
        .map_err(|_| invalid_world_snapshot("invalid player health"))?;
    if !(1..=PLAYER_MAX_HEALTH).contains(&health) {
        return Err(invalid_world_snapshot("player health is out of range"));
    }
    Ok(health)
}

fn invalid_world_snapshot(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn main() {
    if let Err(error) = run() {
        eprintln!("server error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("minimium-server run [--threads 1|2|auto] [--max-players N] [--bind IP] [--game-port PORT] [--data-dir PATH] [--map PATH] [--autosave-seconds N] [--headless]\nminimium-server --version\n--max-players defaults to {MAX_PLAYERS}; --game-port 0 selects an available port (default); --autosave-seconds 0 disables periodic saves (default: {DEFAULT_AUTOSAVE_SECONDS}); --headless disables stdin commands; administer with minimiumctl.");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("minimium-server {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if !args.is_empty() && args[0] != "run" {
        return Err("expected subcommand `run` (try --help)".into());
    }
    let mut lanes = 1usize;
    let mut data_dir = PathBuf::from(".minimium");
    let mut map_path: Option<PathBuf> = None;
    let mut bind_address = String::from("127.0.0.1");
    let mut game_port = 0u16;
    let mut max_players = MAX_PLAYERS;
    let mut autosave_seconds = DEFAULT_AUTOSAVE_SECONDS;
    let mut console_enabled = true;
    let mut index = usize::from(!args.is_empty());
    while index < args.len() {
        match args[index].as_str() {
            "--threads" => {
                index += 1;
                let value = args.get(index).ok_or("--threads requires 1, 2, or auto")?;
                lanes = if value == "auto" {
                    thread::available_parallelism()
                        .map(usize::from)
                        .unwrap_or(1)
                        .min(64)
                } else {
                    value
                        .parse::<usize>()
                        .map_err(|_| "--threads requires 1, 2, or auto")?
                };
                if lanes == 0 || lanes > 64 {
                    return Err("thread lane count must be between 1 and 64".into());
                }
            }
            "--max-players" => {
                index += 1;
                max_players = args
                    .get(index)
                    .ok_or("--max-players requires a number")?
                    .parse()?;
                if !(1..=MAX_PLAYERS).contains(&max_players) {
                    return Err(format!("--max-players must be between 1 and {MAX_PLAYERS}").into());
                }
            }
            "--data-dir" => {
                index += 1;
                data_dir = PathBuf::from(args.get(index).ok_or("--data-dir requires a path")?);
            }
            "--map" => {
                index += 1;
                map_path = Some(PathBuf::from(
                    args.get(index).ok_or("--map requires a path")?,
                ));
            }
            "--bind" => {
                index += 1;
                bind_address = args
                    .get(index)
                    .ok_or("--bind requires an IP address")?
                    .clone();
            }
            "--game-port" => {
                index += 1;
                game_port = args
                    .get(index)
                    .ok_or("--game-port requires a port number")?
                    .parse::<u16>()?;
            }
            "--autosave-seconds" => {
                index += 1;
                autosave_seconds = args
                    .get(index)
                    .ok_or("--autosave-seconds requires a number")?
                    .parse()?;
                if autosave_seconds > 604_800 {
                    return Err("--autosave-seconds must be between 0 and 604800".into());
                }
            }
            "--headless" => console_enabled = false,
            other => return Err(format!("unknown option `{other}` (try --help)").into()),
        }
        index += 1;
    }

    let map = if let Some(path) = map_path.as_deref() {
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

    let _data_lock = ServerDataLock::acquire(&data_dir)?;
    remove_if_exists(&data_dir.join("server.shutdown"))?;
    remove_if_exists(&data_dir.join("server.shutdown.owner"))?;
    let mut player_store = PlayerStore::load(&data_dir, &map)?;
    player_store.active_players.reserve(max_players);
    let mut persistence = PersistenceWorker::spawn(data_dir.clone())?;
    let mut next_player = player_store.next_player_id()?;
    let game_listener = TcpListener::bind((bind_address.as_str(), game_port))?;
    let admin_listener = TcpListener::bind(("127.0.0.1", 0))?;
    game_listener.set_nonblocking(true)?;
    admin_listener.set_nonblocking(true)?;
    let endpoint = ServerEndpoint {
        game_port: game_listener.local_addr()?.port(),
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
    ServerEndpoint::write(&data_dir, &endpoint)?;
    let _endpoint_guard = EndpointGuard {
        data_dir: data_dir.clone(),
        endpoint: endpoint.clone(),
    };
    let mut admin = admin::AdminServer::new(admin_listener, endpoint.token.clone())?;
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
    let mut client_indices_by_player: HashMap<u64, usize> = HashMap::with_capacity(max_players);
    let mut active_inputs: HashMap<u64, CharacterInput> = HashMap::with_capacity(max_players);
    let mut input_last_received: HashMap<u64, Instant> = HashMap::with_capacity(max_players);
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
    let mut next_snapshot = Instant::now() + Duration::from_millis(100);

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
        let mut accepted_this_poll = 0usize;
        loop {
            if accepted_this_poll >= MAX_ACCEPTS_PER_POLL {
                break;
            }
            match game_listener.accept() {
                Ok((stream, _)) => {
                    accepted_this_poll += 1;
                    if let Err(error) = stream.set_nonblocking(true) {
                        eprintln!("client I/O setup failed: {error}");
                        continue;
                    }
                    let rejection = if clients.len() >= max_client_connections
                        || pending_world_inputs.len() + clients.len() >= MAX_PENDING_WORLD_INPUTS
                    {
                        Some(ERROR_SERVER_FULL)
                    } else if clients
                        .iter()
                        .filter(|client| client.player_id.is_none())
                        .count()
                        >= MAX_PENDING_HANDSHAKES
                    {
                        Some(ERROR_BUSY)
                    } else {
                        None
                    };
                    if let Some(error_code) = rejection {
                        let mut stream = stream;
                        if let Ok(frame) = encode_server(&ServerMessage::Error(error_code)) {
                            let _ = stream.write(&frame);
                        }
                        continue;
                    }
                    if let Err(error) = stream.set_nodelay(true) {
                        eprintln!("client I/O setup failed: {error}");
                        continue;
                    }
                    if let Err(error) = stream.set_nonblocking(true) {
                        eprintln!("client I/O setup failed: {error}");
                        continue;
                    }
                    let connected_at = Instant::now();
                    let client = Client {
                        player_id: None,
                        profile_id: None,
                        stream,
                        input: Vec::with_capacity(512),
                        output: Vec::new(),
                        output_offset: 0,
                        output_frames: VecDeque::new(),
                        bytes_in: 0,
                        bytes_out: 0,
                        frames_in: 0,
                        frames_out: 0,
                        encoding_us: 0,
                        replicated_players: RemotePlayers::default(),
                        replication_initialized: false,
                        connected_at,
                        last_message_at: connected_at,
                        last_fire_at: None,
                        last_chat_at: None,
                        pending_health_update: None,
                    };
                    clients.push(client);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    runtime_error = Some(error);
                    break 'server;
                }
            }
        }

        for (index, client) in clients.iter_mut().enumerate() {
            let mut buffer = [0u8; 2048];
            let mut bytes_this_poll = 0usize;
            let mut messages_this_poll = 0usize;
            loop {
                let available =
                    (MAX_READ_BYTES_PER_CLIENT_PER_POLL - bytes_this_poll).min(buffer.len());
                if available == 0 {
                    break;
                }
                match client.stream.read(&mut buffer[..available]) {
                    Ok(0) => {
                        disconnected.push(index);
                        break;
                    }
                    Ok(count) => {
                        bytes_this_poll += count;
                        metrics.bytes_in += count as u64;
                        client.bytes_in += count as u64;
                        client.input.extend_from_slice(&buffer[..count]);
                        loop {
                            match decode_client(&client.input) {
                                Ok(Some((message, frame_bytes))) => {
                                    client.input.drain(..frame_bytes);
                                    client.last_message_at = Instant::now();
                                    messages_this_poll += 1;
                                    metrics.frames_in += 1;
                                    client.frames_in += 1;
                                    messages.push((index, message, Instant::now()));
                                    if messages_this_poll >= MAX_MESSAGES_PER_CLIENT_PER_POLL {
                                        disconnected.push(index);
                                        break;
                                    }
                                }
                                Ok(None) => break,
                                Err(_) => {
                                    disconnected.push(index);
                                    break;
                                }
                            }
                        }
                        if messages_this_poll >= MAX_MESSAGES_PER_CLIENT_PER_POLL
                            || disconnected.last() == Some(&index)
                        {
                            break;
                        }
                        if client.input.len() > MAX_FRAME_BYTES {
                            disconnected.push(index);
                            break;
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        disconnected.push(index);
                        break;
                    }
                }
            }
        }
        for (index, message, decoded_at) in messages.drain(..) {
            if index >= clients.len() || disconnected.contains(&index) {
                continue;
            }
            let player_id = clients[index].player_id;
            match message {
                ClientMessage::Identify(bytes) if player_id.is_none() => {
                    if player_store.active_players.len() >= max_players
                        || pending_world_inputs.len() + clients.len() >= MAX_PENDING_WORLD_INPUTS
                    {
                        let _ = queue_message(
                            &mut clients[index],
                            &mut metrics,
                            ServerMessage::Error(ERROR_SERVER_FULL),
                        );
                        disconnected.push(index);
                        continue;
                    }
                    let profile_id = PlayerProfileId::from_bytes(bytes);
                    let player = match player_store.claim(profile_id, &mut next_player, map.spawn) {
                        Ok(player) => player,
                        Err(error) => {
                            eprintln!("could not claim player profile: {error}");
                            let _ = queue_message(
                                &mut clients[index],
                                &mut metrics,
                                ServerMessage::Error(ERROR_BUSY),
                            );
                            disconnected.push(index);
                            continue;
                        }
                    };
                    clients[index].player_id = Some(player.id);
                    clients[index].profile_id = Some(profile_id);
                    client_indices_by_player.insert(player.id, index);
                    pending_world_inputs.push((
                        WorldInput::Join(
                            player.id,
                            player.name.as_str().to_owned(),
                            player.position,
                            player.health,
                        ),
                        Some(decoded_at),
                    ));
                    if !queue_message(
                        &mut clients[index],
                        &mut metrics,
                        ServerMessage::Welcome {
                            id: player.id,
                            map_fingerprint: map.fingerprint(),
                            health: player.health,
                        },
                    ) {
                        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                        disconnected.push(index);
                    } else {
                        println!("player {} connected", player.id);
                    }
                }
                ClientMessage::Identify(_) => disconnected.push(index),
                ClientMessage::Ping if player_id.is_some() => {
                    if !queue_message(&mut clients[index], &mut metrics, ServerMessage::Pong) {
                        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    }
                }
                ClientMessage::SetName(name) if player_id.is_some() => {
                    if pending_world_inputs.len() + clients.len() < MAX_PENDING_WORLD_INPUTS {
                        pending_world_inputs.push((
                            WorldInput::Rename(player_id.unwrap(), name),
                            Some(decoded_at),
                        ));
                        if !queue_message(
                            &mut clients[index],
                            &mut metrics,
                            ServerMessage::Accepted,
                        ) {
                            metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                        }
                    } else {
                        metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                        let _ = queue_message(
                            &mut clients[index],
                            &mut metrics,
                            ServerMessage::Error(ERROR_BUSY),
                        );
                    }
                }
                ClientMessage::Move { direction, jump } if player_id.is_some() => {
                    if pending_world_inputs.len() + clients.len() < MAX_PENDING_WORLD_INPUTS {
                        pending_world_inputs.push((
                            WorldInput::Move(
                                player_id.unwrap(),
                                CharacterInput { direction, jump },
                            ),
                            Some(decoded_at),
                        ));
                    } else {
                        metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                        let _ = queue_message(
                            &mut clients[index],
                            &mut metrics,
                            ServerMessage::Error(ERROR_BUSY),
                        );
                    }
                }
                ClientMessage::Fire { direction } if player_id.is_some() => {
                    let now = Instant::now();
                    if clients[index].last_fire_at.is_some_and(|last_fire| {
                        now.saturating_duration_since(last_fire) < SHOT_COOLDOWN
                    }) {
                        metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                        if !queue_message(
                            &mut clients[index],
                            &mut metrics,
                            ServerMessage::Error(ERROR_RATE_LIMIT),
                        ) {
                            metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                        }
                    } else if pending_world_inputs.len() + clients.len() < MAX_PENDING_WORLD_INPUTS
                    {
                        clients[index].last_fire_at = Some(now);
                        pending_world_inputs.push((
                            WorldInput::Fire(player_id.unwrap(), direction),
                            Some(decoded_at),
                        ));
                    } else {
                        metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                        if !queue_message(
                            &mut clients[index],
                            &mut metrics,
                            ServerMessage::Error(ERROR_BUSY),
                        ) {
                            metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                        }
                    }
                }
                ClientMessage::Chat(text) if player_id.is_some() => {
                    let now = Instant::now();
                    let rate_limited = clients[index].last_chat_at.is_some_and(|last_chat| {
                        now.saturating_duration_since(last_chat) < CHAT_COOLDOWN
                    }) || last_global_chat_at.is_some_and(|last_chat| {
                        now.saturating_duration_since(last_chat) < GLOBAL_CHAT_COOLDOWN
                    });
                    if rate_limited {
                        metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                        if !queue_message(
                            &mut clients[index],
                            &mut metrics,
                            ServerMessage::Error(ERROR_RATE_LIMIT),
                        ) {
                            metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                        }
                    } else if pending_world_inputs.len() + clients.len() < MAX_PENDING_WORLD_INPUTS
                    {
                        clients[index].last_chat_at = Some(now);
                        last_global_chat_at = Some(now);
                        pending_world_inputs
                            .push((WorldInput::Chat(player_id.unwrap(), text), Some(decoded_at)));
                    } else {
                        metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                        if !queue_message(
                            &mut clients[index],
                            &mut metrics,
                            ServerMessage::Error(ERROR_BUSY),
                        ) {
                            metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                        }
                    }
                }
                ClientMessage::Quit if player_id.is_some() => {
                    let _ = queue_message(&mut clients[index], &mut metrics, ServerMessage::Bye);
                    disconnected.push(index);
                }
                _ => disconnected.push(index),
            }
        }
        let activity_check_at = Instant::now();
        for (index, client) in clients.iter().enumerate() {
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
        for (index, client) in clients.iter_mut().enumerate() {
            if let Some(health) = client.pending_health_update {
                if !queue_message(client, &mut metrics, ServerMessage::HealthUpdate(health)) {
                    metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                } else {
                    client.pending_health_update = None;
                }
            }
            metrics.max_client_input_bytes = metrics.max_client_input_bytes.max(client.input.len());
            metrics.max_client_output_bytes = metrics
                .max_client_output_bytes
                .max(client.output.len().saturating_sub(client.output_offset));
            let flushed = network::flush(
                &mut client.stream,
                &client.output,
                &mut client.output_offset,
            );
            metrics.bytes_out += flushed.written as u64;
            client.bytes_out += flushed.written as u64;
            account_output(client, flushed.written);
            if flushed.error.is_some() {
                disconnected.push(index);
            }
        }
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
                if let (Some(player_id), Some(_profile_id)) = (client.player_id, client.profile_id)
                {
                    client_indices_by_player.remove(&player_id);
                    // Capture the last committed position before releasing the profile:
                    // a reconnect may arrive before the queued Leave reaches the next tick.
                    if let Some(player) = world.player(player_id) {
                        if let Err(error) = player_store.update_from_player(player) {
                            eprintln!("could not capture disconnect state: {error}");
                        }
                    }
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
                    let _ = write!(reply.text, " max_players={max_players} map_fingerprint={:016x} tick_ms=50 snapshot_ms=100 autosave_seconds={autosave_seconds} protocol_version=8 save_version=3", map.fingerprint());
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
            for (input, decoded_at) in pending_world_inputs.drain(..) {
                if let Some(decoded_at) = decoded_at {
                    metrics
                        .input_wait
                        .record(Instant::now().saturating_duration_since(decoded_at));
                }
                match input {
                    WorldInput::Join(id, name, position, health) => {
                        world.join_at_with_health(id, name, position, health);
                        active_inputs.insert(id, CharacterInput::default());
                        input_last_received.insert(id, Instant::now());
                    }
                    WorldInput::Leave(id) => {
                        if let Some(player) = world.player(id) {
                            if let Err(error) = player_store.update_from_player(player) {
                                eprintln!("could not capture player {} before leave: {error}", id);
                            }
                        }
                        world.leave(id);
                        active_inputs.remove(&id);
                        input_last_received.remove(&id);
                    }
                    WorldInput::Move(id, input) => {
                        let current = active_inputs.entry(id).or_default();
                        current.direction = input.direction;
                        current.jump |= input.jump;
                        input_last_received.insert(id, Instant::now());
                    }
                    WorldInput::Fire(id, direction) => {
                        metrics.shots_fired = metrics.shots_fired.saturating_add(1);
                        let Some(outcome) = world.fire_hitscan(id, direction) else {
                            continue;
                        };
                        if let Some(target_id) = outcome.target_id {
                            metrics.shots_hit = metrics.shots_hit.saturating_add(1);
                            if outcome.killed {
                                metrics.player_kills = metrics.player_kills.saturating_add(1);
                            }
                            if let Some(target_index) =
                                client_indices_by_player.get(&target_id).copied()
                            {
                                if let Some(health) = world.player_health(target_id) {
                                    clients[target_index].pending_health_update = Some(health);
                                }
                            }
                        }
                        if let Some(shooter_index) = client_indices_by_player.get(&id).copied() {
                            let message = ServerMessage::ShotResult {
                                target_id: outcome.target_id,
                                remaining_health: outcome.remaining_health,
                                killed: outcome.killed,
                            };
                            if !queue_message(&mut clients[shooter_index], &mut metrics, message) {
                                metrics.dropped_messages =
                                    metrics.dropped_messages.saturating_add(1);
                            }
                        }
                    }
                    WorldInput::Chat(id, text) => {
                        if !player_store.active_players.contains(&id) {
                            continue;
                        }
                        let Some(sender_name) = player_store.name_for_player(id) else {
                            continue;
                        };
                        let message = ServerMessage::Chat {
                            sender_id: id,
                            sender_name: sender_name.to_owned(),
                            text,
                        };
                        let encoding_started = Instant::now();
                        let encoded = encode_server(&message);
                        metrics.encoding.record(encoding_started.elapsed());
                        let Ok(frame) = encoded else {
                            metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                            continue;
                        };
                        for client in &mut clients {
                            if !queue_encoded_frame(client, &frame, &mut metrics) {
                                metrics.dropped_messages =
                                    metrics.dropped_messages.saturating_add(1);
                            }
                        }
                    }
                    WorldInput::Rename(id, name) => {
                        world.rename_player(id, name.clone());
                        player_store.rename(id, name);
                    }
                }
            }
            let now = Instant::now();
            for (id, last_received) in &input_last_received {
                if now.duration_since(*last_received) > Duration::from_millis(250) {
                    if let Some(input) = active_inputs.get_mut(id) {
                        input.direction = [0.0; 2];
                    }
                }
            }
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
            let recipients: Vec<_> = clients
                .iter()
                .enumerate()
                .filter_map(|(index, client)| client.player_id.map(|id| (index, id)))
                .collect();
            if !recipients.is_empty() {
                let snapshot = world.network_snapshot();
                let snapshots = executor.map(recipients, move |(index, recipient)| {
                    let character_state = snapshot.character_state(recipient).unwrap_or_default();
                    let mut players = RemotePlayers::default();
                    snapshot.visit_nearby::<MAX_VISIBLE_PLAYERS>(
                        recipient,
                        MAX_VISIBLE_PLAYERS,
                        |player| {
                            let _ = players.push(RemotePlayer {
                                id: player.id,
                                name: player.name.clone(),
                                position: player.position,
                            });
                        },
                    );
                    (index, character_state, players)
                });
                for (recipient, character_state, current_players) in snapshots {
                    if let Some(client) = clients.get_mut(recipient) {
                        let full_snapshot = !client.replication_initialized;
                        let (players, removed_players) = if full_snapshot {
                            (current_players.clone(), RemotePlayerIds::default())
                        } else {
                            diff_remote_players(&client.replicated_players, &current_players)
                        };
                        let message = ServerMessage::Snapshot {
                            server_time_ms,
                            full_snapshot,
                            self_position: character_state.position,
                            self_velocity: character_state.velocity,
                            self_grounded: character_state.grounded,
                            players,
                            removed_players,
                        };
                        if queue_message(client, &mut metrics, message) {
                            client.replicated_players = current_players;
                            client.replication_initialized = true;
                        } else {
                            metrics.dropped_snapshots = metrics.dropped_snapshots.saturating_add(1);
                        }
                    }
                }
            }
            metrics.snapshots = metrics.snapshots.saturating_add(1);
            metrics.snapshot_prep.record(snapshot_started.elapsed());
            next_snapshot += Duration::from_millis(100);
            let now = Instant::now();
            if now > next_snapshot + Duration::from_millis(100) {
                let interval = Duration::from_millis(100);
                let late_by = now.duration_since(next_snapshot).as_nanos();
                metrics.missed_snapshots = metrics
                    .missed_snapshots
                    .saturating_add((late_by / interval.as_nanos()) as u64);
                next_snapshot = now + interval;
            }
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

    // Queue stop response before final storage; bound the flush even for a nonreading peer.
    admin.reject_pending("server stopping");
    let drain_deadline = Instant::now() + Duration::from_secs(2);
    while admin.has_pending_output() && Instant::now() < drain_deadline {
        match admin.poll(Instant::now()) {
            Ok(requests) => {
                for request in requests {
                    admin.respond(request.id, false, "server stopping");
                }
            }
            Err(error) => {
                runtime_error.get_or_insert(error);
                break;
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
    for client in &mut clients {
        let _ = client.stream.shutdown(std::net::Shutdown::Both);
    }
    let save_result = match player_store.save_snapshot(&world, map.fingerprint()) {
        Ok(snapshot) => persistence.shutdown(Some(snapshot)),
        Err(error) => persistence.shutdown(None).and(Err(error)),
    };
    let endpoint_result = endpoint.remove_if_owned(&data_dir);
    let shutdown_result = save_result
        .and(endpoint_result)
        .and(runtime_error.map_or(Ok(()), Err));
    let status_result = write_shutdown_status(&data_dir, &endpoint, &shutdown_result);
    shutdown_result?;
    status_result?;
    println!("server stopped cleanly");
    Ok(())
}

fn queue_message(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
) -> bool {
    if client.output_frames.len() >= MAX_QUEUED_OUTPUT_FRAMES {
        return false;
    }
    prepare_output(client);
    if client.output.len() + MAX_SERVER_FRAME_BYTES > MAX_QUEUED_OUTPUT_BYTES {
        return false;
    }
    client.output.reserve_exact(MAX_SERVER_FRAME_BYTES);
    let frame_start = client.output.len();
    let encoding_started = Instant::now();
    let result = encode_server_into(&mut client.output, &message);
    let encoding_time = encoding_started.elapsed();
    metrics.encoding.record(encoding_time);
    client.encoding_us = client
        .encoding_us
        .saturating_add(encoding_time.as_micros().min(u64::MAX as u128) as u64);
    match result {
        Ok(()) if client.output.len() <= MAX_QUEUED_OUTPUT_BYTES => {
            client
                .output_frames
                .push_back((client.output.len() - frame_start, Instant::now()));
            client.frames_out += 1;
            metrics.frames_out += 1;
            true
        }
        Ok(()) => {
            client.output.truncate(frame_start);
            false
        }
        Err(error) => {
            client.output.truncate(frame_start);
            eprintln!("could not encode game message: {error}");
            false
        }
    }
}

fn queue_encoded_frame(client: &mut Client, frame: &[u8], metrics: &mut RuntimeMetrics) -> bool {
    if client.output_frames.len() >= MAX_QUEUED_OUTPUT_FRAMES {
        return false;
    }
    prepare_output(client);
    if client.output.len().saturating_add(frame.len()) > MAX_QUEUED_OUTPUT_BYTES {
        return false;
    }
    client.output.reserve(frame.len());
    client.output.extend_from_slice(frame);
    client
        .output_frames
        .push_back((frame.len(), Instant::now()));
    client.frames_out += 1;
    metrics.frames_out += 1;
    true
}

fn diff_remote_players(
    previous: &RemotePlayers,
    current: &RemotePlayers,
) -> (RemotePlayers, RemotePlayerIds) {
    let mut changed = RemotePlayers::default();
    for player in current.iter() {
        let differs = previous
            .get(player.id)
            .is_none_or(|old| old.name != player.name || old.position != player.position);
        if differs {
            let _ = changed.push(player.clone());
        }
    }

    let mut removed = RemotePlayerIds::default();
    for previous_player in previous.iter() {
        if current.get(previous_player.id).is_none() {
            let _ = removed.push(previous_player.id);
        }
    }
    (changed, removed)
}

fn prepare_output(client: &mut Client) {
    if client.output_offset > 0 {
        let pending = client.output.len() - client.output_offset;
        client.output.copy_within(client.output_offset.., 0);
        client.output.truncate(pending);
        client.output_offset = 0;
    }
}

fn account_output(client: &mut Client, mut written: usize) {
    while written > 0 {
        let Some((remaining, _)) = client.output_frames.front_mut() else {
            break;
        };
        let consumed = written.min(*remaining);
        *remaining -= consumed;
        written -= consumed;
        if *remaining == 0 {
            client.output_frames.pop_front();
        }
    }
}

struct EndpointGuard {
    data_dir: PathBuf,
    endpoint: ServerEndpoint,
}
impl Drop for EndpointGuard {
    fn drop(&mut self) {
        if let Err(error) = self.endpoint.remove_if_owned(&self.data_dir) {
            eprintln!("could not remove server endpoint: {error}");
        }
    }
}
