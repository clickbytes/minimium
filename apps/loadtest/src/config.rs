use std::{env, path::PathBuf, thread};

use super::scenarios::Scenario;

#[derive(Clone, Copy)]
pub(super) enum WorkerSelection {
    One,
    Two,
    Auto,
}

impl WorkerSelection {
    fn worker_limit(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Auto => thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                .min(2),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
            Self::Auto => "auto",
        }
    }
}

pub(super) struct StartupConfig {
    pub(super) address: String,
    pub(super) clients: usize,
    pub(super) seconds: u64,
    pub(super) worker_selection: WorkerSelection,
    pub(super) output_format: OutputFormat,
    pub(super) scenario: Scenario,
    pub(super) cpu_label: Option<String>,
    pub(super) cpu_mhz: Option<u32>,
    pub(super) scenario_id: Option<String>,
    pub(super) server_data_dir: Option<PathBuf>,
}

#[derive(Clone, Copy)]
pub(super) enum OutputFormat {
    Text,
    Json,
}

pub(super) enum ParseOutcome {
    Help,
    Config(StartupConfig),
}

impl StartupConfig {
    pub(super) fn worker_count(&self) -> usize {
        self.worker_selection.worker_limit().min(self.clients)
    }

    pub(super) fn thread_mode(&self) -> &'static str {
        self.worker_selection.as_str()
    }
}

pub(super) fn parse_env() -> Result<ParseOutcome, Box<dyn std::error::Error>> {
    parse_args(env::args().skip(1))
}

fn parse_args(
    args: impl IntoIterator<Item = String>,
) -> Result<ParseOutcome, Box<dyn std::error::Error>> {
    let args: Vec<String> = args.into_iter().collect();
    let mut address = None;
    let mut clients = 100usize;
    let mut seconds = 10u64;
    let mut worker_selection = WorkerSelection::Auto;
    let mut output_format = OutputFormat::Text;
    let mut scenario = Scenario::Heartbeat;
    let mut cpu_label = None;
    let mut cpu_mhz = None;
    let mut scenario_id = None;
    let mut server_data_dir = None;
    let mut seen = std::collections::HashSet::new();
    let mut index = 0;
    while index < args.len() {
        if !seen.insert(args[index].clone()) {
            return Err(format!("duplicate option: {}", args[index]).into());
        }
        match args[index].as_str() {
            "--cpu-label" | "--scenario-id" | "--server-data-dir" | "--cpu-mhz" => {
                let option = &args[index];
                index += 1;
                let value = args
                    .get(index)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| format!("{option} requires a value"))?;
                if value.len() > 512 {
                    return Err(format!("{option} exceeds 512 bytes").into());
                }
                match option.as_str() {
                    "--cpu-label" => cpu_label = Some(value.clone()),
                    "--scenario-id" => scenario_id = Some(value.clone()),
                    "--server-data-dir" => server_data_dir = Some(PathBuf::from(value)),
                    _ => {
                        let mhz: u32 = value.parse()?;
                        if mhz == 0 {
                            return Err("--cpu-mhz must be positive".into());
                        }
                        cpu_mhz = Some(mhz);
                    }
                }
            }
            "--address" => {
                index += 1;
                address = Some(
                    args.get(index)
                        .ok_or("--address requires HOST:PORT")?
                        .clone(),
                );
            }
            "--clients" => {
                index += 1;
                clients = args
                    .get(index)
                    .ok_or("--clients requires a number")?
                    .parse()?;
            }
            "--seconds" => {
                index += 1;
                seconds = args
                    .get(index)
                    .ok_or("--seconds requires a number")?
                    .parse()?;
            }
            "--threads" => {
                index += 1;
                worker_selection = match args
                    .get(index)
                    .ok_or("--threads requires 1, 2, or auto")?
                    .as_str()
                {
                    "1" => WorkerSelection::One,
                    "2" => WorkerSelection::Two,
                    "auto" => WorkerSelection::Auto,
                    _ => return Err("threads must be 1, 2, or auto".into()),
                };
            }
            "--format" => {
                index += 1;
                output_format = match args
                    .get(index)
                    .ok_or("--format requires text or json")?
                    .as_str()
                {
                    "text" => OutputFormat::Text,
                    "json" => OutputFormat::Json,
                    _ => return Err("format must be text or json".into()),
                };
            }
            "--scenario" => {
                index += 1;
                scenario = match args.get(index).ok_or("--scenario requires heartbeat, movement, combat, chat, clustered, or dispersed")?.as_str() {
                    "heartbeat" => Scenario::Heartbeat,
                    "movement" => Scenario::Movement,
                    "combat" => Scenario::Combat,
                    "chat" => Scenario::Chat,
                    "clustered" => Scenario::Clustered,
                    "dispersed" => Scenario::Dispersed,
                    _ => return Err("scenario must be heartbeat, movement, combat, chat, clustered, or dispersed".into()),
                };
            }
            "--help" | "-h" => return Ok(ParseOutcome::Help),
            other => return Err(format!("unknown option: {other}").into()),
        }
        index += 1;
    }
    let address = address.ok_or("--address is required")?;
    if clients == 0 || clients > 10_000 {
        return Err("clients must be between 1 and 10000".into());
    }
    if !(1..=86400).contains(&seconds) {
        return Err("seconds must be between 1 and 86400".into());
    }
    Ok(ParseOutcome::Config(StartupConfig {
        address,
        clients,
        seconds,
        worker_selection,
        output_format,
        scenario,
        cpu_label,
        cpu_mhz,
        scenario_id,
        server_data_dir,
    }))
}

pub(super) const HELP: &str = "minimium-loadtest --address HOST:PORT [--scenario heartbeat|movement|combat|chat|clustered|dispersed] [--clients N] [--seconds N] [--threads 1|2|auto] [--format text|json] [--scenario-id TEXT] [--cpu-label TEXT] [--cpu-mhz N] [--server-data-dir PATH]";
