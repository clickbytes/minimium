use std::{error::Error, path::PathBuf, thread};

pub const MAX_PLAYERS: usize = 512;
pub const DEFAULT_AUTOSAVE_SECONDS: u64 = 300;

#[derive(Clone, Copy)]
pub enum LaneSelection {
    Fixed(usize),
    Auto,
}

impl LaneSelection {
    pub fn count(self) -> usize {
        match self {
            Self::Fixed(count) => count,
            Self::Auto => thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                .min(64),
        }
    }
}

pub struct ServerConfig {
    pub lanes: LaneSelection,
    pub data_dir: PathBuf,
    pub map_path: Option<PathBuf>,
    pub bind_address: String,
    pub game_port: u16,
    pub max_players: usize,
    pub autosave_seconds: u64,
    pub console_enabled: bool,
}

impl ServerConfig {
    pub fn parse(args: &[String]) -> Result<Self, Box<dyn Error>> {
        if !args.is_empty() && args[0] != "run" {
            return Err("expected subcommand `run` (try --help)".into());
        }
        let mut config = Self {
            lanes: LaneSelection::Fixed(1),
            data_dir: PathBuf::from(minimium_core::files::DEFAULT_SERVER_DATA_DIR),
            map_path: None,
            bind_address: String::from("127.0.0.1"),
            game_port: 0,
            max_players: MAX_PLAYERS,
            autosave_seconds: DEFAULT_AUTOSAVE_SECONDS,
            console_enabled: true,
        };
        let mut index = usize::from(!args.is_empty());
        while index < args.len() {
            match args[index].as_str() {
                "--threads" => {
                    index += 1;
                    let value = args.get(index).ok_or("--threads requires 1, 2, or auto")?;
                    config.lanes = if value == "auto" {
                        LaneSelection::Auto
                    } else {
                        let count = value
                            .parse::<usize>()
                            .map_err(|_| "--threads requires 1, 2, or auto")?;
                        if !(1..=64).contains(&count) {
                            return Err("thread lane count must be between 1 and 64".into());
                        }
                        LaneSelection::Fixed(count)
                    };
                }
                "--max-players" => {
                    index += 1;
                    config.max_players = args
                        .get(index)
                        .ok_or("--max-players requires a number")?
                        .parse()?;
                    if !(1..=MAX_PLAYERS).contains(&config.max_players) {
                        return Err(
                            format!("--max-players must be between 1 and {MAX_PLAYERS}").into()
                        );
                    }
                }
                "--data-dir" => {
                    index += 1;
                    config.data_dir =
                        PathBuf::from(args.get(index).ok_or("--data-dir requires a path")?);
                }
                "--map" => {
                    index += 1;
                    config.map_path = Some(PathBuf::from(
                        args.get(index).ok_or("--map requires a path")?,
                    ));
                }
                "--bind" => {
                    index += 1;
                    config.bind_address = args
                        .get(index)
                        .ok_or("--bind requires an IP address")?
                        .clone();
                }
                "--game-port" => {
                    index += 1;
                    config.game_port = args
                        .get(index)
                        .ok_or("--game-port requires a port number")?
                        .parse()?;
                }
                "--autosave-seconds" => {
                    index += 1;
                    config.autosave_seconds = args
                        .get(index)
                        .ok_or("--autosave-seconds requires a number")?
                        .parse()?;
                    if config.autosave_seconds > 604_800 {
                        return Err("--autosave-seconds must be between 0 and 604800".into());
                    }
                }
                "--headless" => config.console_enabled = false,
                other => return Err(format!("unknown option `{other}` (try --help)").into()),
            }
            index += 1;
        }
        Ok(config)
    }
}
