use std::{
    collections::BTreeMap,
    io,
    net::IpAddr,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_CONFIG_OUTPUT: usize = 64 * 1024;
const MAX_DIAGNOSTICS: usize = 2048;

/// Normalized settings used for one locally owned server launch.
#[derive(Clone, Debug)]
pub struct LocalServerLaunchConfig {
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub parent_run_id: String,
    pub diagnostic_logger: Option<minimium_diagnostics::DomainLogger>,
    pub data_dir_explicit: bool,
    pub config_path: Option<PathBuf>,
    pub settings: BTreeMap<String, String>,
    pub environment: BTreeMap<String, String>,
    pub client_map_path: Option<PathBuf>,
    pub client_world_definition_path: Option<PathBuf>,
}

impl LocalServerLaunchConfig {
    pub(crate) fn resolve(
        &self,
        server_executable: &Path,
        deadline: Instant,
    ) -> io::Result<ResolvedLaunch> {
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "server config preview deadline expired",
            ));
        }
        let mut command = server_command(server_executable, self);
        command.args(["config", "show", "--sources", "--format", "json"]);
        self.append_config_inputs(&mut command);
        let output = run_preview(command, deadline)?;
        if output.stdout_exceeded {
            return Err(io::Error::other("server config preview exceeded 64 KiB"));
        }
        if !output.status.success() {
            let detail = bounded_text(&output.stderr, MAX_DIAGNOSTICS);
            return Err(io::Error::other(if detail.is_empty() {
                format!("server config preview failed with {}", output.status)
            } else {
                format!("server config preview failed: {detail}")
            }));
        }
        let view: ConfigView = serde_json::from_slice(&output.stdout).map_err(|error| {
            io::Error::other(format!("invalid server config preview JSON: {error}"))
        })?;
        if view.schema_version != 1 {
            return Err(io::Error::other(format!(
                "unsupported server config preview schema {}",
                view.schema_version
            )));
        }
        if view.settings.is_empty() {
            return Err(io::Error::other(
                "server config preview returned no settings",
            ));
        }
        let data_dir = if self.data_dir_explicit {
            self.data_dir.clone()
        } else {
            view.settings
                .get("server.data_dir")
                .filter(|setting| setting.source != "default")
                .map(|setting| PathBuf::from(&setting.effective))
                .unwrap_or_else(|| self.data_dir.clone())
        };
        validate_local_constraints(&view)?;
        validate_client_world_inputs(self, &view)?;
        Ok(ResolvedLaunch { view, data_dir })
    }

    pub(crate) fn unapplied_on_attach(&self, resolved: &ResolvedLaunch) -> Vec<String> {
        resolved
            .view
            .settings
            .iter()
            .filter(|(key, setting)| {
                setting.source != "default"
                    && !(self.data_dir_explicit && key.as_str() == "server.data_dir")
            })
            .map(|(key, _)| key.clone())
            .collect()
    }

    pub(crate) fn append_run_arguments(
        &self,
        command: &mut Command,
        resolved: &ResolvedLaunch,
        attempt_id: &str,
    ) {
        command.arg("run");
        if !resolved
            .view
            .settings
            .get("server.headless")
            .is_some_and(|setting| setting.effective == "true")
        {
            command.arg("--headless");
        }
        let data_dir_is_resolved_setting = resolved
            .view
            .settings
            .get("server.data_dir")
            .is_some_and(|setting| setting.source != "default")
            && !self.data_dir_explicit;
        if !data_dir_is_resolved_setting || self.data_dir_explicit {
            command.arg("--data-dir").arg(&resolved.data_dir);
        }
        command.arg("--log-dir").arg(&self.log_dir);
        command.arg("--parent-run-id").arg(&self.parent_run_id);
        command.arg("--startup-attempt-id").arg(attempt_id);
        let dev_mode = resolved
            .view
            .settings
            .get("server.dev-mode")
            .is_some_and(|setting| setting.effective == "true");
        if dev_mode {
            command.arg("--dev");
        }
        if let Some(path) = &self.config_path {
            command.arg("--config").arg(path);
        }
        for (key, value) in &self.settings {
            if dev_mode && key == "server.dev-mode" {
                continue;
            }
            command.arg("--set").arg(format!("{key}={value}"));
        }
    }

    fn append_config_inputs(&self, command: &mut Command) {
        if let Some(path) = &self.config_path {
            command.arg("--config").arg(path);
        }
        for (key, value) in &self.settings {
            command.arg("--set").arg(format!("{key}={value}"));
        }
    }

    pub(crate) fn prepare_command(&self, command: &mut Command) {
        clear_server_environment(command);
        command.envs(&self.environment);
    }
}

struct PreviewOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stdout_exceeded: bool,
    stderr: Vec<u8>,
}

struct CapturedStream {
    bytes: Vec<u8>,
    exceeded: bool,
}

fn run_preview(mut command: Command, deadline: Instant) -> io::Result<PreviewOutput> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("preview stdout is piped");
    let stdout_reader = match spawn_reader(stdout, MAX_CONFIG_OUTPUT, "server-config-stdout") {
        Ok(reader) => reader,
        Err(error) => {
            kill_and_reap(&mut child);
            return Err(error);
        }
    };
    let stderr = child.stderr.take().expect("preview stderr is piped");
    let stderr_reader = match spawn_reader(stderr, MAX_DIAGNOSTICS, "server-config-stderr") {
        Ok(reader) => reader,
        Err(error) => {
            kill_and_reap(&mut child);
            let _ = stdout_reader.join();
            return Err(error);
        }
    };
    let status = loop {
        if Instant::now() >= deadline {
            kill_and_reap(&mut child);
            let _ = join_reader(stdout_reader);
            let _ = join_reader(stderr_reader);
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "server config preview exceeded startup deadline",
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(5)),
            Err(error) => {
                kill_and_reap(&mut child);
                let _ = join_reader(stdout_reader);
                let _ = join_reader(stderr_reader);
                return Err(error);
            }
        }
    };
    let stdout = join_reader(stdout_reader)?;
    let stderr = join_reader(stderr_reader)?;
    Ok(PreviewOutput {
        status,
        stdout: stdout.bytes,
        stdout_exceeded: stdout.exceeded,
        stderr: stderr.bytes,
    })
}

fn spawn_reader<R: io::Read + Send + 'static>(
    mut reader: R,
    limit: usize,
    name: &str,
) -> io::Result<JoinHandle<io::Result<CapturedStream>>> {
    thread::Builder::new().name(name.to_owned()).spawn(move || {
        let mut captured = Vec::with_capacity(limit.min(8192));
        let mut buffer = [0; 4096];
        let mut exceeded = false;
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            let remaining = limit.saturating_sub(captured.len());
            let retained = count.min(remaining);
            captured.extend_from_slice(&buffer[..retained]);
            exceeded |= retained != count;
        }
        Ok(CapturedStream {
            bytes: captured,
            exceeded,
        })
    })
}

fn join_reader(reader: JoinHandle<io::Result<CapturedStream>>) -> io::Result<CapturedStream> {
    reader
        .join()
        .map_err(|_| io::Error::other("server config preview output reader panicked"))?
}

fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub(crate) struct ResolvedLaunch {
    view: ConfigView,
    pub(crate) data_dir: PathBuf,
}

fn validate_local_constraints(view: &ConfigView) -> io::Result<()> {
    let bind = view
        .settings
        .get("server.bind")
        .ok_or_else(|| io::Error::other("server config preview omitted server.bind"))?;
    if bind.effective.parse::<IpAddr>().ok() != Some(IpAddr::from([127, 0, 0, 1])) {
        return Err(io::Error::other(format!(
            "locally started servers must bind to 127.0.0.1 (resolved server.bind={})",
            bind.effective
        )));
    }
    let headless = view
        .settings
        .get("server.headless")
        .ok_or_else(|| io::Error::other("server config preview omitted server.headless"))?;
    if headless.effective != "true" && headless.source != "default" {
        return Err(io::Error::other(
            "local server setting server.headless must be true",
        ));
    }
    Ok(())
}

fn validate_client_world_inputs(
    config: &LocalServerLaunchConfig,
    view: &ConfigView,
) -> io::Result<()> {
    for (key, client_path, flag) in [
        ("world.map", config.client_map_path.as_deref(), "--map PATH"),
        (
            "world.definition",
            config.client_world_definition_path.as_deref(),
            "--world-definition PATH",
        ),
    ] {
        let resolved = view
            .settings
            .get(key)
            .ok_or_else(|| io::Error::other(format!("server config preview omitted {key}")))?;
        if resolved.effective.is_empty() {
            if client_path.is_some() {
                return Err(io::Error::other(format!(
                    "client {flag} is missing from the resolved server configuration"
                )));
            }
        } else if client_path != Some(Path::new(&resolved.effective)) {
            return Err(io::Error::other(format!(
                "resolved {key}={} must match the client world input; pass {flag}",
                resolved.effective
            )));
        }
    }
    // The preview includes the same settings and environment passed to the
    // locally owned server. A selected seed or planet preset is therefore a
    // valid resolved input, not a mismatch with a separate client world.
    Ok(())
}

fn server_command(server_executable: &Path, config: &LocalServerLaunchConfig) -> Command {
    let mut command = Command::new(server_executable);
    config.prepare_command(&mut command);
    command
}

fn clear_server_environment(command: &mut Command) {
    command.env_remove("MINIMIUM_SERVER_THREADS");
    command.env_remove("MINIMIUM_SERVER_DATA_DIR");
    command.env_remove("MINIMIUM_SERVER_BIND");
    command.env_remove("MINIMIUM_SERVER_GAME_PORT");
    command.env_remove("MINIMIUM_SERVER_MAX_PLAYERS");
    command.env_remove("MINIMIUM_SERVER_AUTOSAVE_INTERVAL");
    command.env_remove("MINIMIUM_SERVER_HEADLESS");
    command.env_remove("MINIMIUM_SERVER_SEED");
    command.env_remove("MINIMIUM_SERVER_PLANET_CONFIG");
    command.env_remove("MINIMIUM_SERVER_CLIMATE_RELIEF_INFLUENCE");
}

fn bounded_text(bytes: &[u8], limit: usize) -> String {
    String::from_utf8_lossy(&bytes[..bytes.len().min(limit)])
        .trim()
        .to_owned()
}

#[derive(serde::Deserialize)]
struct ConfigView {
    schema_version: u32,
    settings: BTreeMap<String, ResolvedSetting>,
}

#[derive(serde::Deserialize)]
struct ResolvedSetting {
    effective: String,
    source: String,
}
