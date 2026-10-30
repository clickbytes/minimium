mod accept;
mod admin;
mod commands;
mod config;
mod connection;
mod executor;
mod lifecycle;
mod metrics;
mod network;
mod receive;
mod replication;
mod sector_streaming;
mod send;
mod simulation;
mod startup;
mod storage;
mod terminal;
mod world_inputs;
mod world_location;
mod world_registry;
use config::{DEFAULT_AUTOSAVE_SECONDS, MAX_PLAYERS};
#[cfg(feature = "allocation-metrics")]
use minimium_telemetry::allocations;
#[cfg(feature = "allocation-metrics")]
#[global_allocator]
static ALLOCATOR: allocations::CountingAllocator = allocations::CountingAllocator;
use std::{env, time::Duration};

const TICK: Duration = Duration::from_millis(50);
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(100);
const MAX_PENDING_HANDSHAKES: usize = 64;
const MAX_PENDING_WORLD_INPUTS: usize = 4096;
const CLIENT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const CLIENT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const SHOT_COOLDOWN: Duration = Duration::from_millis(250);
const CHAT_COOLDOWN: Duration = Duration::from_secs(1);
const GLOBAL_CHAT_COOLDOWN: Duration = Duration::from_millis(50);
mod runtime;

fn main() {
    if let Err(error) = run() {
        eprintln!("server error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!("minimium-server run [--threads 1..64|auto] [--max-players N] [--bind IP] [--game-port PORT] [--data-dir PATH] [--map PATH | --world-definition PATH] [--autosave-seconds N] [--headless]\nminimium-server --version\n--max-players defaults to {MAX_PLAYERS}; --game-port 0 selects an available port (default); --autosave-seconds 0 disables periodic saves (default: {DEFAULT_AUTOSAVE_SECONDS}); --headless disables stdin commands; administer with minimiumctl.");
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("minimium-server {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let config = config::ServerConfig::parse(&args)?;
    let startup = startup::initialize(
        &config.data_dir,
        config.map_path.as_deref(),
        config.world_definition_path.as_deref(),
        &config.bind_address,
        config.game_port,
        config.max_players,
    )?;
    runtime::ServerRuntime::new(config, startup)?.run()?;
    Ok(())
}
