use crate::environment::quality::SkyQuality;
use std::path::PathBuf;

const DEFAULT_LOCAL_DATA_DIR: &str = ".minimium-local";

/// Server worker lane selection accepted by the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadLane {
    One,
    Two,
    Auto,
}

impl ThreadLane {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "1" => Ok(Self::One),
            "2" => Ok(Self::Two),
            "auto" => Ok(Self::Auto),
            _ => Err("--threads must be 1, 2, or auto".to_owned()),
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
            Self::Auto => "auto",
        }
    }
}

/// Command-line options shared by the desktop and text clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientOptions {
    pub connect: Option<String>,
    pub map_path: Option<PathBuf>,
    pub world_definition_path: Option<PathBuf>,
    pub demo: bool,
    pub require_earth: bool,
    pub console: bool,
    pub help: bool,
    pub threads: ThreadLane,
    pub sky_quality: SkyQuality,
    pub profile_dir: Option<PathBuf>,
    pub server_data_dir: Option<PathBuf>,
}

impl ClientOptions {
    /// Parse client arguments, excluding the executable name.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            connect: None,
            map_path: None,
            world_definition_path: None,
            demo: false,
            require_earth: false,
            console: false,
            help: false,
            threads: ThreadLane::One,
            sky_quality: SkyQuality::Standard,
            profile_dir: None,
            server_data_dir: None,
        };
        let mut seen_connect = false;
        let mut seen_map = false;
        let mut seen_world_definition = false;
        let mut seen_demo = false;
        let mut seen_require_earth = false;
        let mut seen_console = false;
        let mut seen_help = false;
        let mut seen_threads = false;
        let mut seen_sky_quality = false;
        let mut seen_profile_dir = false;
        let mut seen_server_data_dir = false;
        let mut args = args.into_iter().peekable();

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--connect" => {
                    reject_duplicate(&mut seen_connect, "--connect")?;
                    options.connect = Some(required_value(&mut args, "--connect")?);
                }
                "--map" => {
                    reject_duplicate(&mut seen_map, "--map")?;
                    options.map_path = Some(PathBuf::from(required_value(&mut args, "--map")?));
                }
                "--world-definition" => {
                    reject_duplicate(&mut seen_world_definition, "--world-definition")?;
                    options.world_definition_path = Some(PathBuf::from(required_value(
                        &mut args,
                        "--world-definition",
                    )?));
                }
                "--profile-dir" => {
                    reject_duplicate(&mut seen_profile_dir, "--profile-dir")?;
                    options.profile_dir =
                        Some(PathBuf::from(required_value(&mut args, "--profile-dir")?));
                }
                "--server-data-dir" => {
                    reject_duplicate(&mut seen_server_data_dir, "--server-data-dir")?;
                    options.server_data_dir = Some(PathBuf::from(required_value(
                        &mut args,
                        "--server-data-dir",
                    )?));
                }
                "--threads" => {
                    reject_duplicate(&mut seen_threads, "--threads")?;
                    let value = required_value(&mut args, "--threads")?;
                    options.threads = ThreadLane::parse(&value)?;
                }
                "--sky-quality" => {
                    reject_duplicate(&mut seen_sky_quality, "--sky-quality")?;
                    let value = required_value(&mut args, "--sky-quality")?;
                    options.sky_quality = SkyQuality::parse(&value)
                        .map_err(|_| "--sky-quality must be low or standard".to_owned())?;
                }
                "--demo" => {
                    reject_duplicate(&mut seen_demo, "--demo")?;
                    options.demo = true;
                }
                "--require-earth" => {
                    reject_duplicate(&mut seen_require_earth, "--require-earth")?;
                    options.require_earth = true;
                }
                "--console" => {
                    reject_duplicate(&mut seen_console, "--console")?;
                    options.console = true;
                }
                "--help" | "-h" => {
                    reject_duplicate(&mut seen_help, "--help")?;
                    options.help = true;
                }
                _ => return Err(format!("unknown client option `{arg}` (try --help)")),
            }
        }

        if options.demo && options.connect.is_some() {
            return Err("--demo cannot be used with --connect".to_owned());
        }
        if options.demo && options.console {
            return Err("--demo cannot be used with --console".to_owned());
        }
        if options.require_earth && options.demo {
            return Err("--require-earth cannot be used with --demo".to_owned());
        }
        if options.require_earth && options.console {
            return Err("--require-earth requires the desktop client".to_owned());
        }
        if options.map_path.is_some() && options.world_definition_path.is_some() {
            return Err("--map and --world-definition cannot be used together".to_owned());
        }

        Ok(options)
    }

    /// Resolve independent profile and server storage directories.
    pub fn paths(&self, map_fingerprint: u64) -> (PathBuf, PathBuf) {
        let default_dir = if self.map_path.is_some() || self.world_definition_path.is_some() {
            PathBuf::from(DEFAULT_LOCAL_DATA_DIR).join(format!("map-{map_fingerprint:016x}"))
        } else {
            PathBuf::from(DEFAULT_LOCAL_DATA_DIR)
        };
        (
            self.profile_dir
                .clone()
                .unwrap_or_else(|| default_dir.clone()),
            self.server_data_dir.clone().unwrap_or(default_dir),
        )
    }
}

fn required_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    match args.next() {
        Some(value) if !value.starts_with("--") && value != "-h" => Ok(value),
        _ => Err(format!("{option} requires a value")),
    }
}

fn reject_duplicate(seen: &mut bool, option: &str) -> Result<(), String> {
    if *seen {
        Err(format!("{option} may only be specified once"))
    } else {
        *seen = true;
        Ok(())
    }
}
