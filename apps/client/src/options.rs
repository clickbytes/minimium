use std::path::PathBuf;

/// Command-line options shared by the desktop and text clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientOptions {
    pub connect: Option<String>,
    pub map_path: Option<PathBuf>,
    pub demo: bool,
    pub console: bool,
    pub help: bool,
    pub threads: String,
    pub profile_dir: Option<PathBuf>,
    pub server_data_dir: Option<PathBuf>,
}

impl ClientOptions {
    /// Parse client arguments, excluding the executable name.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut options = Self {
            connect: None,
            map_path: None,
            demo: false,
            console: false,
            help: false,
            threads: "1".to_owned(),
            profile_dir: None,
            server_data_dir: None,
        };
        let mut seen_connect = false;
        let mut seen_map = false;
        let mut seen_demo = false;
        let mut seen_console = false;
        let mut seen_help = false;
        let mut seen_threads = false;
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
                    if !matches!(value.as_str(), "1" | "2" | "auto") {
                        return Err("--threads must be 1, 2, or auto".to_owned());
                    }
                    options.threads = value;
                }
                "--demo" => {
                    reject_duplicate(&mut seen_demo, "--demo")?;
                    options.demo = true;
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

        Ok(options)
    }

    /// Resolve independent profile and server storage directories.
    pub fn paths(&self, map_fingerprint: u64) -> (PathBuf, PathBuf) {
        let default_dir = if self.map_path.is_some() {
            PathBuf::from(".minimium-local").join(format!("map-{map_fingerprint:016x}"))
        } else {
            PathBuf::from(".minimium-local")
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
