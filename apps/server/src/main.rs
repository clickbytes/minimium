use minimium_core::{
    commands::{ServerCommand, SERVER_HELP},
    endpoint::{new_admin_token, ServerEndpoint},
    executor::Executor,
    identity::PlayerProfileId,
    world::{CharacterInput, World, WorldMap},
};
use minimium_protocol::{
    decode_client, encode_server, encode_server_into, ClientMessage, RemotePlayer, RemotePlayers,
    ServerMessage, ERROR_BUSY, ERROR_SERVER_FULL, HEADER_BYTES, MAX_FRAME_BYTES,
    MAX_VISIBLE_PLAYERS,
};
use std::{
    collections::{HashMap, HashSet},
    env,
    fs::{self, File},
    io::{self, BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::{mpsc, Arc},
    thread,
    time::{Duration, Instant},
};

const TICK: Duration = Duration::from_millis(50);
const MAX_PLAYERS: usize = 512;
const MAX_READ_BYTES_PER_CLIENT_PER_POLL: usize = 1024;
const SAMPLE_MAP: &str = include_str!("../../../games/sample/maps/demo.map");
const MAX_MESSAGES_PER_CLIENT_PER_POLL: usize = 32;
const MAX_PENDING_WORLD_INPUTS: usize = 4096;
const MAX_QUEUED_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_SERVER_FRAME_BYTES: usize = HEADER_BYTES + 26 + MAX_VISIBLE_PLAYERS * 45;
const DURATION_BUCKETS_MICROS: [u64; 8] = [50, 100, 250, 500, 1_000, 2_500, 5_000, 50_000];
const MAX_STORED_PLAYERS: usize = 100_000;
const MAX_WORLD_SNAPSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_SAVED_COORDINATE: f32 = 1_000_000.0;
const PLAYER_STORE_SHARDS: usize = 256;

struct Client {
    player_id: Option<u64>,
    profile_id: Option<PlayerProfileId>,
    stream: TcpStream,
    input: Vec<u8>,
    output: Vec<u8>,
    output_offset: usize,
}

enum WorldInput {
    Join(u64, String, [f32; 3]),
    Leave(u64),
    Move(u64, CharacterInput),
    Rename(u64, String),
}

#[derive(Clone, Copy)]
struct StoredPlayer {
    id: u64,
    name: StoredName,
    position: [f32; 3],
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

struct AdminContext<'a> {
    token: &'a str,
    world: &'a World,
    player_store: &'a mut PlayerStore,
    persistence: &'a mut PersistenceWorker,
    map_fingerprint: u64,
    lanes: usize,
    metrics: &'a RuntimeMetrics,
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

#[derive(Default)]
struct DurationStats {
    samples: u64,
    total_nanoseconds: u128,
    maximum_nanoseconds: u64,
    buckets: [u64; DURATION_BUCKETS_MICROS.len() + 1],
}

impl DurationStats {
    fn record(&mut self, duration: Duration) {
        let nanoseconds = duration.as_nanos().min(u128::from(u64::MAX)) as u64;
        let microseconds = duration.as_micros().min(u128::from(u64::MAX)) as u64;
        let bucket = DURATION_BUCKETS_MICROS
            .iter()
            .position(|limit| microseconds <= *limit)
            .unwrap_or(DURATION_BUCKETS_MICROS.len());
        self.samples = self.samples.saturating_add(1);
        self.total_nanoseconds = self
            .total_nanoseconds
            .saturating_add(u128::from(nanoseconds));
        self.maximum_nanoseconds = self.maximum_nanoseconds.max(nanoseconds);
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
    }

    fn average_microseconds(&self) -> u64 {
        if self.samples == 0 {
            return 0;
        }
        (self.total_nanoseconds / u128::from(self.samples) / 1_000).min(u128::from(u64::MAX)) as u64
    }

    fn percentile_microseconds(&self, percentile: u64) -> u64 {
        if self.samples == 0 {
            return 0;
        }
        let rank = self.samples.saturating_mul(percentile).saturating_add(99) / 100;
        let mut cumulative = 0u64;
        for (index, count) in self.buckets.iter().enumerate() {
            cumulative = cumulative.saturating_add(*count);
            if cumulative >= rank {
                return DURATION_BUCKETS_MICROS
                    .get(index)
                    .copied()
                    .unwrap_or(self.maximum_nanoseconds / 1_000);
            }
        }
        self.maximum_nanoseconds / 1_000
    }
}

#[derive(Default)]
struct RuntimeMetrics {
    ticks: u64,
    missed_ticks: u64,
    snapshots: u64,
    missed_snapshots: u64,
    bytes_in: u64,
    bytes_out: u64,
    dropped_messages: u64,
    dropped_snapshots: u64,
    rejected_inputs: u64,
    last_tick: Duration,
    tick: DurationStats,
    loop_work: DurationStats,
    network_io: DurationStats,
    input_apply: DurationStats,
    simulation: DurationStats,
    snapshot_prep: DurationStats,
    max_pending_inputs: usize,
    max_client_input_bytes: usize,
    max_client_output_bytes: usize,
}

impl RuntimeMetrics {
    fn summary(&self, players: usize) -> String {
        format!(
            "ticks={} missed_ticks={} snapshots={} missed_snapshots={} players={} bytes_in={} bytes_out={} dropped_messages={} dropped_snapshots={} rejected_inputs={} tick_last_us={} tick_avg_us={} tick_p95_us={} tick_p99_us={} loop_p95_us={} network_p95_us={} input_p95_us={} simulation_p95_us={} snapshot_p95_us={} max_pending_inputs={} max_client_input_bytes={} max_client_output_bytes={}",
            self.ticks,
            self.missed_ticks,
            self.snapshots,
            self.missed_snapshots,
            players,
            self.bytes_in,
            self.bytes_out,
            self.dropped_messages,
            self.dropped_snapshots,
            self.rejected_inputs,
            self.last_tick.as_micros(),
            self.tick.average_microseconds(),
            self.tick.percentile_microseconds(95),
            self.tick.percentile_microseconds(99),
            self.loop_work.percentile_microseconds(95),
            self.network_io.percentile_microseconds(95),
            self.input_apply.percentile_microseconds(95),
            self.simulation.percentile_microseconds(95),
            self.snapshot_prep.percentile_microseconds(95),
            self.max_pending_inputs,
            self.max_client_input_bytes,
            self.max_client_output_bytes,
        )
    }
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
        if version != Some("2") {
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
            if fields.next().is_some() || !map.can_place_player(position) {
                return Err(invalid_world_snapshot(
                    "saved player position overlaps map geometry",
                ));
            }
            let shard = player_store_shard(profile_id);
            let duplicate_profile = Arc::make_mut(&mut store.players[shard])
                .insert(profile_id, StoredPlayer { id, name, position })
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
            writeln!(file, "MINIMIUM_WORLD\t2\t{:016x}", self.map_fingerprint)?;
            for (_, profile_id, player) in players {
                writeln!(
                    file,
                    "{}\t{}\t{}\t{:.6}\t{:.6}\t{:.6}",
                    player.id,
                    profile_id.to_hex(),
                    player.name.as_str(),
                    player.position[0],
                    player.position[1],
                    player.position[2]
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
    admin_port: u16,
    result: &io::Result<()>,
) -> io::Result<()> {
    let path = data_dir.join("server.shutdown");
    let temporary_path = data_dir.join("server.shutdown.tmp");
    let write_result = (|| {
        let mut file = File::create(&temporary_path)?;
        match result {
            Ok(()) => writeln!(file, "{admin_port}\tOK")?,
            Err(error) => writeln!(file, "{admin_port}\tERR\t{}", compact_error(error))?,
        }
        file.sync_all()?;
        atomic_replace_file(&temporary_path, &path)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(temporary_path);
    }
    write_result
}

#[cfg(windows)]
fn atomic_replace_file(source: &std::path::Path, destination: &std::path::Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(source: *const u16, destination: *const u16, flags: u32) -> i32;
    }

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;
    let source: Vec<u16> = source.as_os_str().encode_wide().chain([0]).collect();
    let destination: Vec<u16> = destination.as_os_str().encode_wide().chain([0]).collect();
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn atomic_replace_file(source: &std::path::Path, destination: &std::path::Path) -> io::Result<()> {
    fs::rename(source, destination)
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
        println!("minimium-server run [--threads 1|2|auto] [--bind IP] [--data-dir PATH] [--map PATH] [--headless]\nminimium-server --version\n--headless disables stdin commands; administer with minimiumctl.");
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
                } else {
                    value
                        .parse::<usize>()
                        .map_err(|_| "--threads requires 1, 2, or auto")?
                };
                if lanes == 0 || lanes > 64 {
                    return Err("thread lane count must be between 1 and 64".into());
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

    fs::create_dir_all(&data_dir)?;
    remove_if_exists(&data_dir.join("server.shutdown"))?;
    let mut player_store = PlayerStore::load(&data_dir, &map)?;
    let mut persistence = PersistenceWorker::spawn(data_dir.clone())?;
    let mut next_player = player_store.next_player_id()?;
    let game_listener = TcpListener::bind((bind_address.as_str(), 0))?;
    let admin_listener = TcpListener::bind(("127.0.0.1", 0))?;
    game_listener.set_nonblocking(true)?;
    admin_listener.set_nonblocking(true)?;
    let endpoint = ServerEndpoint {
        game_port: game_listener.local_addr()?.port(),
        admin_port: admin_listener.local_addr()?.port(),
        token: new_admin_token()?,
    };
    ServerEndpoint::write(&data_dir, &endpoint)?;
    println!(
        "Minimium server started: game={bind_address}:{} admin=127.0.0.1:{} lanes={lanes} map={:016x}",
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

    let (console_tx, console_rx) = mpsc::sync_channel::<String>(64);
    let _console_thread = if console_enabled {
        Some(
            thread::Builder::new()
                .name("minimium-console-input".into())
                .spawn(move || {
                    for line in io::stdin().lock().lines() {
                        match line {
                            Ok(line) => {
                                if console_tx.send(line).is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                })?,
        )
    } else {
        None
    };
    let executor = Executor::new(lanes);
    let mut world = World::default();
    world.load_map(&map);
    let mut clients: Vec<Client> = Vec::new();
    let mut active_inputs: HashMap<u64, CharacterInput> = HashMap::with_capacity(MAX_PLAYERS);
    let mut input_last_received: HashMap<u64, Instant> = HashMap::with_capacity(MAX_PLAYERS);
    let mut pending_world_inputs = Vec::new();
    let mut messages: Vec<(usize, ClientMessage)> = Vec::with_capacity(MAX_PLAYERS);
    let mut disconnected: Vec<usize> = Vec::with_capacity(MAX_PLAYERS);
    let mut stop = false;
    let mut metrics = RuntimeMetrics::default();
    let mut next_tick = Instant::now() + TICK;
    let mut next_snapshot = Instant::now() + Duration::from_millis(100);

    while !stop {
        persistence.poll_completions();
        messages.clear();
        disconnected.clear();
        let loop_started = Instant::now();
        while let Ok(line) = console_rx.try_recv() {
            match ServerCommand::parse(&line) {
                Ok(command) => {
                    stop |= execute_command(
                        command,
                        &mut world,
                        &mut player_store,
                        &mut persistence,
                        map.fingerprint(),
                        lanes,
                        &metrics,
                    )?
                }
                Err(error) => eprintln!("command error: {error}"),
            }
        }

        loop {
            match admin_listener.accept() {
                Ok((stream, _)) => {
                    let result = handle_admin(
                        stream,
                        AdminContext {
                            token: &endpoint.token,
                            world: &world,
                            player_store: &mut player_store,
                            persistence: &mut persistence,
                            map_fingerprint: map.fingerprint(),
                            lanes,
                            metrics: &metrics,
                        },
                    );
                    match result {
                        Ok((reply, should_stop)) => {
                            stop |= should_stop;
                            if !reply.is_empty() {
                                println!("{reply}");
                            }
                        }
                        Err(error) => eprintln!("admin request failed: {error}"),
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.into()),
            }
        }

        let network_started = Instant::now();
        loop {
            match game_listener.accept() {
                Ok((stream, _)) => {
                    if clients.len() >= MAX_PLAYERS {
                        let mut stream = stream;
                        if let Ok(frame) = encode_server(&ServerMessage::Error(ERROR_SERVER_FULL)) {
                            let _ = stream.write_all(&frame);
                        }
                        continue;
                    }
                    stream.set_nodelay(true)?;
                    stream.set_nonblocking(true)?;
                    let client = Client {
                        player_id: None,
                        profile_id: None,
                        stream,
                        input: Vec::with_capacity(512),
                        output: Vec::new(),
                        output_offset: 0,
                    };
                    clients.push(client);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => return Err(error.into()),
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
                        client.input.extend_from_slice(&buffer[..count]);
                        loop {
                            match decode_client(&client.input) {
                                Ok(Some((message, frame_bytes))) => {
                                    client.input.drain(..frame_bytes);
                                    messages_this_poll += 1;
                                    messages.push((index, message));
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
        for (index, message) in messages.drain(..) {
            if index >= clients.len() {
                continue;
            }
            let player_id = clients[index].player_id;
            match message {
                ClientMessage::Identify(bytes) if player_id.is_none() => {
                    let profile_id = PlayerProfileId::from_bytes(bytes);
                    let player = match player_store.claim(profile_id, &mut next_player, map.spawn) {
                        Ok(player) => player,
                        Err(error) => {
                            eprintln!("could not claim player profile: {error}");
                            let _ = queue_message(
                                &mut clients[index],
                                ServerMessage::Error(ERROR_BUSY),
                            );
                            disconnected.push(index);
                            continue;
                        }
                    };
                    clients[index].player_id = Some(player.id);
                    clients[index].profile_id = Some(profile_id);
                    pending_world_inputs.push(WorldInput::Join(
                        player.id,
                        player.name.as_str().to_owned(),
                        player.position,
                    ));
                    if !queue_message(
                        &mut clients[index],
                        ServerMessage::Welcome {
                            id: player.id,
                            map_fingerprint: map.fingerprint(),
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
                    if !queue_message(&mut clients[index], ServerMessage::Pong) {
                        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                    }
                }
                ClientMessage::SetName(name) if player_id.is_some() => {
                    if pending_world_inputs.len() < MAX_PENDING_WORLD_INPUTS {
                        pending_world_inputs.push(WorldInput::Rename(player_id.unwrap(), name));
                        if !queue_message(&mut clients[index], ServerMessage::Accepted) {
                            metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
                        }
                    } else {
                        metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                        let _ =
                            queue_message(&mut clients[index], ServerMessage::Error(ERROR_BUSY));
                    }
                }
                ClientMessage::Move { direction, jump } if player_id.is_some() => {
                    if pending_world_inputs.len() < MAX_PENDING_WORLD_INPUTS {
                        pending_world_inputs.push(WorldInput::Move(
                            player_id.unwrap(),
                            CharacterInput { direction, jump },
                        ));
                    } else {
                        metrics.rejected_inputs = metrics.rejected_inputs.saturating_add(1);
                        let _ =
                            queue_message(&mut clients[index], ServerMessage::Error(ERROR_BUSY));
                    }
                }
                ClientMessage::Quit if player_id.is_some() => {
                    let _ = queue_message(&mut clients[index], ServerMessage::Bye);
                    disconnected.push(index);
                }
                _ => disconnected.push(index),
            }
        }
        disconnected.sort_unstable();
        disconnected.dedup();
        for client in &mut clients {
            metrics.max_client_input_bytes = metrics.max_client_input_bytes.max(client.input.len());
            metrics.max_client_output_bytes = metrics
                .max_client_output_bytes
                .max(client.output.len().saturating_sub(client.output_offset));
            match flush(client) {
                Ok(written) => metrics.bytes_out += written as u64,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(_) => client.output_offset = client.output.len(),
            }
        }
        for index in disconnected.drain(..).rev() {
            if index < clients.len() {
                let client = clients.swap_remove(index);
                if let (Some(player_id), Some(_profile_id)) = (client.player_id, client.profile_id)
                {
                    player_store.release(player_id);
                    pending_world_inputs.push(WorldInput::Leave(player_id));
                    println!("player {player_id} disconnected");
                }
            }
        }
        metrics.network_io.record(network_started.elapsed());
        metrics.max_pending_inputs = metrics.max_pending_inputs.max(pending_world_inputs.len());

        if Instant::now() >= next_tick {
            let tick_started = Instant::now();
            let input_apply_started = Instant::now();
            for input in pending_world_inputs.drain(..) {
                match input {
                    WorldInput::Join(id, name, position) => {
                        world.join_at(id, name, position);
                        active_inputs.insert(id, CharacterInput::default());
                        input_last_received.insert(id, Instant::now());
                    }
                    WorldInput::Leave(id) => {
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
            let simulation_started = Instant::now();
            world.step_players(&active_inputs, TICK.as_secs_f32());
            metrics.simulation.record(simulation_started.elapsed());
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
            let snapshot_started = Instant::now();
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
                    snapshot.visit_nearby(recipient, MAX_VISIBLE_PLAYERS, |player| {
                        let _ = players.push(RemotePlayer {
                            id: player.id,
                            name: player.name.clone(),
                            position: player.position,
                        });
                    });
                    (
                        index,
                        ServerMessage::Snapshot {
                            self_position: character_state.position,
                            self_velocity: character_state.velocity,
                            self_grounded: character_state.grounded,
                            players,
                        },
                    )
                });
                for (recipient, snapshot) in snapshots {
                    if let Some(client) = clients.get_mut(recipient) {
                        if !queue_message(client, snapshot) {
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
        thread::sleep(poll_interval.min(until_tick).min(until_snapshot));
    }

    for client in &mut clients {
        let _ = client.stream.shutdown(std::net::Shutdown::Both);
    }
    let save_result = match player_store.save_snapshot(&world, map.fingerprint()) {
        Ok(snapshot) => persistence.shutdown(Some(snapshot)),
        Err(error) => persistence.shutdown(None).and(Err(error)),
    };
    let endpoint_result = fs::remove_file(ServerEndpoint::file_path(&data_dir)).or_else(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            Ok(())
        } else {
            Err(error)
        }
    });
    let shutdown_result = save_result.and(endpoint_result);
    let status_result = write_shutdown_status(&data_dir, endpoint.admin_port, &shutdown_result);
    shutdown_result?;
    status_result?;
    println!("server stopped cleanly");
    Ok(())
}

fn queue_message(client: &mut Client, message: ServerMessage) -> bool {
    prepare_output(client);
    if client.output.len() + MAX_SERVER_FRAME_BYTES > MAX_QUEUED_OUTPUT_BYTES {
        return false;
    }
    client.output.reserve_exact(MAX_SERVER_FRAME_BYTES);
    let frame_start = client.output.len();
    match encode_server_into(&mut client.output, &message) {
        Ok(()) if client.output.len() <= MAX_QUEUED_OUTPUT_BYTES => true,
        Ok(()) => {
            client.output.truncate(frame_start);
            false
        }
        Err(error) => {
            eprintln!("could not encode game message: {error}");
            false
        }
    }
}

fn prepare_output(client: &mut Client) {
    if client.output_offset > 0 {
        let pending = client.output.len() - client.output_offset;
        client.output.copy_within(client.output_offset.., 0);
        client.output.truncate(pending);
        client.output_offset = 0;
    }
}

fn flush(client: &mut Client) -> io::Result<usize> {
    let mut written = 0;
    while client.output_offset < client.output.len() {
        match client.stream.write(&client.output[client.output_offset..]) {
            Ok(0) => break,
            Ok(count) => {
                client.output_offset += count;
                written += count;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    if client.output_offset == client.output.len() {
        client.output.clear();
        client.output_offset = 0;
    }
    Ok(written)
}

fn handle_admin(mut stream: TcpStream, context: AdminContext<'_>) -> io::Result<(String, bool)> {
    let AdminContext {
        token,
        world,
        player_store,
        persistence,
        map_fingerprint,
        lanes,
        metrics,
    } = context;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut auth = String::new();
    let mut command_line = String::new();
    reader.read_line(&mut auth)?;
    reader.read_line(&mut command_line)?;
    if auth.trim() != format!("AUTH {token}") {
        stream.write_all(b"ERR unauthorized\n")?;
        return Ok((String::new(), false));
    }
    let (reply, should_stop) = match ServerCommand::parse(command_line.trim()) {
        Ok(ServerCommand::Status) => (
            format!(
                "OK running players={} lanes={lanes} ticks={} {}",
                world.player_count(),
                metrics.ticks,
                persistence.summary()
            ),
            false,
        ),
        Ok(ServerCommand::Players) => {
            let players = world.snapshot();
            let value = if players.is_empty() {
                "no players".to_owned()
            } else {
                players
                    .iter()
                    .map(|p| {
                        format!(
                            "{}:{}@{:.1},{:.1},{:.1}",
                            p.id, p.name, p.position[0], p.position[1], p.position[2]
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            (format!("OK {value}"), false)
        }
        Ok(ServerCommand::Metrics) => (
            format!("OK {}", metrics.summary(world.player_count())),
            false,
        ),
        Ok(ServerCommand::Save) => {
            let result = persistence.queue_world_save(player_store, world, map_fingerprint);
            match result {
                Ok(()) => (
                    "OK world save queued; use status to check completion".into(),
                    false,
                ),
                Err(error) => (format!("ERR world save: {error}"), false),
            }
        }
        Ok(ServerCommand::Stop) => ("OK stopping".into(), true),
        Ok(ServerCommand::Help) => (format!("OK {SERVER_HELP}"), false),
        Err(error) => (format!("ERR {error}"), false),
    };
    writeln!(stream, "{reply}")?;
    Ok((reply, should_stop))
}

fn execute_command(
    command: ServerCommand,
    world: &mut World,
    player_store: &mut PlayerStore,
    persistence: &mut PersistenceWorker,
    map_fingerprint: u64,
    lanes: usize,
    metrics: &RuntimeMetrics,
) -> io::Result<bool> {
    match command {
        ServerCommand::Help => println!("{SERVER_HELP}"),
        ServerCommand::Status => println!(
            "running players={} lanes={lanes} ticks={} {}",
            world.player_count(),
            metrics.ticks,
            persistence.summary()
        ),
        ServerCommand::Players => println!("players: {}", world.player_count()),
        ServerCommand::Metrics => println!("{}", metrics.summary(world.player_count())),
        ServerCommand::Save => {
            let result = persistence.queue_world_save(player_store, world, map_fingerprint);
            match result {
                Ok(()) => println!("world save queued; use `status` to check completion"),
                Err(error) => eprintln!("could not queue world save: {error}"),
            }
        }
        ServerCommand::Stop => return Ok(true),
    }
    Ok(false)
}
