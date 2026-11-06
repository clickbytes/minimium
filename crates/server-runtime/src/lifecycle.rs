use crate::{
    admin::AdminServer,
    connection::Client,
    environment::EnvironmentClock,
    storage::{compact_error, PersistenceWorker, PlayerStore},
    world_registry::WorldRegistry,
};
use minimium_environment::time::MonotonicMs;
use minimium_platform::{
    admin_client::{SHUTDOWN_FILE, SHUTDOWN_OWNER_FILE},
    endpoint::ServerEndpoint,
    files::atomic_replace_file,
};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub(crate) struct ShutdownContext<'a> {
    pub admin: &'a mut AdminServer,
    pub clients: &'a mut [Client],
    pub player_store: &'a mut PlayerStore,
    pub persistence: &'a mut PersistenceWorker,
    pub environment_clock: &'a mut EnvironmentClock,
    pub server_started: Instant,
    pub worlds: &'a WorldRegistry,
    pub map_fingerprint: u64,
    pub endpoint: &'a ServerEndpoint,
    pub data_dir: &'a Path,
    pub runtime_error: Option<io::Error>,
}

pub(crate) fn shutdown(mut context: ShutdownContext<'_>) -> io::Result<()> {
    drain_admin_requests(context.admin, &mut context.runtime_error);
    close_client_sockets(context.clients);
    let snapshot = capture_final_snapshot(
        context.server_started,
        context.environment_clock,
        context.worlds,
        context.map_fingerprint,
        context.player_store,
    );
    let save_result = persist_final_snapshot(context.persistence, snapshot);
    finish_shutdown(
        context.data_dir,
        context.endpoint,
        context.runtime_error,
        save_result,
    )
}

fn drain_admin_requests(admin: &mut AdminServer, runtime_error: &mut Option<io::Error>) {
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
}

fn close_client_sockets(clients: &mut [Client]) {
    for client in clients {
        let _ = client.stream.shutdown(std::net::Shutdown::Both);
    }
}

fn capture_final_snapshot(
    server_started: Instant,
    environment_clock: &mut EnvironmentClock,
    worlds: &WorldRegistry,
    map_fingerprint: u64,
    player_store: &mut PlayerStore,
) -> io::Result<crate::storage::SaveSnapshot> {
    let capture_at = Instant::now();
    let monotonic_ms = u64::try_from(
        capture_at
            .saturating_duration_since(server_started)
            .as_millis(),
    )
    .map(MonotonicMs::new)
    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "server monotonic time overflow"));
    let snapshot = monotonic_ms
        .and_then(|at| {
            environment_clock.capture_saved_state(at).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("environment clock: {error}"),
                )
            })
        })
        .and_then(|saved_clock| {
            let input = worlds.snapshot_for_save(map_fingerprint)?;
            player_store.save_snapshot(input, saved_clock)
        });
    snapshot
}

fn persist_final_snapshot(
    persistence: &mut PersistenceWorker,
    snapshot: io::Result<crate::storage::SaveSnapshot>,
) -> io::Result<()> {
    match snapshot {
        Ok(snapshot) => persistence.shutdown(Some(snapshot)),
        Err(error) => {
            if let Err(cleanup_error) = persistence.shutdown(None) {
                eprintln!("storage shutdown after snapshot failure: {cleanup_error}");
            }
            Err(error)
        }
    }
}

fn finish_shutdown(
    data_dir: &Path,
    endpoint: &ServerEndpoint,
    runtime_error: Option<io::Error>,
    save_result: io::Result<()>,
) -> io::Result<()> {
    let endpoint_result = endpoint.remove_if_owned(data_dir);
    let mut primary_error = runtime_error;
    for (operation, result) in [
        ("final save", save_result),
        ("endpoint cleanup", endpoint_result),
    ] {
        if let Err(error) = result {
            if primary_error.is_some() {
                eprintln!("{operation} also failed: {error}");
            } else {
                primary_error = Some(error);
            }
        }
    }
    let shutdown_result = primary_error.map_or(Ok(()), Err);
    let status_result = write_shutdown_status(data_dir, endpoint, &shutdown_result);
    if let Err(error) = &status_result {
        if shutdown_result.is_err() {
            eprintln!("shutdown status write also failed: {error}");
        }
    }
    shutdown_result?;
    status_result?;
    println!("server stopped cleanly");
    Ok(())
}

pub(crate) fn remove_if_exists(path: &std::path::Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn write_shutdown_status(
    data_dir: &std::path::Path,
    endpoint: &ServerEndpoint,
    result: &io::Result<()>,
) -> io::Result<()> {
    let admin_port = endpoint.admin_port;
    let path = data_dir.join(SHUTDOWN_FILE);
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
            &data_dir.join(SHUTDOWN_OWNER_FILE),
        )?;
        atomic_replace_file(&temporary_path, &path)?;
        Ok(())
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(temporary_path);
    }
    write_result
}

pub(crate) struct EndpointGuard {
    pub(crate) data_dir: PathBuf,
    pub(crate) endpoint: ServerEndpoint,
}
impl Drop for EndpointGuard {
    fn drop(&mut self) {
        if let Err(error) = self.endpoint.remove_if_owned(&self.data_dir) {
            eprintln!("could not remove server endpoint: {error}");
        }
    }
}
