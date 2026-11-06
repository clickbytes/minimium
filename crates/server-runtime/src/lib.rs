//! Authoritative world lifecycle, ticking, networking and persistence runtime.
mod accept;
mod admin;
mod commands;
pub mod composition;
mod config;
mod connection;
mod content;
mod environment;
mod executor;
mod lifecycle;
mod metrics;
#[allow(dead_code)]
mod modules;
mod receive;
mod replication;
mod sector_streaming;
mod send;
mod server_command;
mod simulation;
mod startup;
mod storage;
#[cfg(test)]
mod storage_tests;
mod terminal;
mod world_inputs;
mod world_registry;
use composition::ServerComposition;
use config::{DEFAULT_AUTOSAVE_SECONDS, MAX_PLAYERS};
use std::time::Duration;

const TICK: Duration = Duration::from_millis(minimium_simulation::SIMULATION_STEP_MILLISECONDS);
const SNAPSHOT_INTERVAL: Duration = Duration::from_millis(100);
const MAX_PENDING_HANDSHAKES: usize = 64;
const MAX_PENDING_WORLD_INPUTS: usize = 4096;
const CLIENT_IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const CLIENT_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const CHAT_COOLDOWN: Duration = Duration::from_secs(1);
const GLOBAL_CHAT_COOLDOWN: Duration = Duration::from_millis(50);
mod runtime;

/// Parses server arguments and runs the runtime with application-owned content.
pub fn run(
    args: &[String],
    composition: ServerComposition,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "minimium-server run [--seed DECIMAL|0xHEX] [--planet-config PATH] \\
             [--environment-config PATH] [--climate-scenario PATH] [--climate-relief-influence 0..1] [--threads 1..64|auto] [--max-players N] \\
             [--material-package PATH] [--material-http-port PORT] \\
             [--bind IP] [--game-port PORT] [--data-dir PATH] \\
             [--map PATH | --world-definition PATH] [--autosave-seconds N] [--headless]\n\\
             minimium-server --version\n\\
             --max-players defaults to {MAX_PLAYERS}; --game-port 0 selects an available port (default); \\
             --autosave-seconds 0 disables periodic saves (default: {DEFAULT_AUTOSAVE_SECONDS}); \\
             --environment-config initializes clock mode/date/scale for a new world only; \\
             --headless disables stdin commands; administer with minimiumctl."
        );
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version") {
        println!("minimium-server {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let config = config::ServerConfig::parse(args)?;
    commands::initialize_modules();
    let startup = startup::initialize(&config, composition.world_composer)?;
    runtime::ServerRuntime::new(config, startup, composition.combat)?.run()?;
    Ok(())
}
