use crate::{
    admin::AdminServer,
    connection::Client,
    content::ContentServer,
    environment::EnvironmentClock,
    executor::Executor,
    sector_streaming::SectorGenerationPool,
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
    pub shutdown_request: &'a crate::composition::ShutdownRequest,
    pub admin: &'a mut AdminServer,
    pub clients: &'a mut [Client],
    pub player_store: &'a mut PlayerStore,
    pub persistence: &'a mut PersistenceWorker,
    pub content: &'a mut ContentServer,
    pub sector_workers: &'a mut SectorGenerationPool,
    pub executor: &'a mut Executor,
    pub environment_clock: &'a mut EnvironmentClock,
    pub server_started: Instant,
    pub worlds: &'a WorldRegistry,
    pub map_fingerprint: u64,
    pub endpoint: &'a ServerEndpoint,
    pub data_dir: &'a Path,
    pub logger: &'a minimium_diagnostics::DomainLogger,
    pub runtime_error: Option<io::Error>,
}

pub(crate) fn shutdown(mut context: ShutdownContext<'_>) -> io::Result<()> {
    // One process-wide 30-second deadline; the final two seconds belong to diagnostics.
    let log_deadline = context.shutdown_request.begin_shutdown();
    let deadline = log_deadline - Duration::from_secs(2);
    context.admin.stop_admission();
    drain_admin_requests(context.admin, &mut context.runtime_error, deadline);
    close_client_sockets(context.clients);
    let content_deadline = (Instant::now() + Duration::from_secs(2)).min(deadline);
    let mut unfinished = context
        .content
        .shutdown(content_deadline.saturating_duration_since(Instant::now()));
    unfinished.extend(context.sector_workers.shutdown_until(deadline));
    unfinished.extend(context.executor.shutdown_until(deadline));
    let snapshot = capture_final_snapshot(
        context.server_started,
        context.environment_clock,
        context.worlds,
        context.map_fingerprint,
        context.player_store,
        context.logger,
        deadline,
    );
    let save_result =
        persist_final_snapshot(context.persistence, snapshot, deadline, context.logger);
    if !unfinished.is_empty() {
        crate::diagnostics::emit(
            context.logger,
            minimium_diagnostics::Level::Error,
            "server.shutdown_workers_unfinished",
            &[("workers", unfinished.join(","))],
        );
        context.runtime_error.get_or_insert_with(|| {
            io::Error::new(
                io::ErrorKind::TimedOut,
                format!(
                    "workers exceeded shutdown deadline: {}",
                    unfinished.join(",")
                ),
            )
        });
    }
    finish_shutdown(
        context.data_dir,
        context.endpoint,
        context.runtime_error,
        save_result,
        context.logger,
    )
}

fn drain_admin_requests(
    admin: &mut AdminServer,
    runtime_error: &mut Option<io::Error>,
    deadline: Instant,
) {
    admin.reject_pending("server stopping");
    while admin.has_pending_output() && Instant::now() < deadline {
        match admin.poll(Instant::now()) {
            Ok(requests) => {
                for request in requests {
                    admin.respond(
                        request.id,
                        false,
                        Some("server.stopping"),
                        "server stopping",
                    );
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
    logger: &minimium_diagnostics::DomainLogger,
    deadline: Instant,
) -> io::Result<crate::storage::SaveSnapshot> {
    if Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "final snapshot capture missed shutdown deadline",
        ));
    }
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
            environment_clock
                .capture_saved_state(at)
                .map_err(io::Error::other)
        })
        .and_then(|saved_clock| {
            let input = worlds.snapshot_for_save(map_fingerprint)?;
            player_store
                .save_snapshot(input, saved_clock)
                .map_err(io::Error::from)
        });
    if let Err(error) = &snapshot {
        crate::diagnostics::emit(
            logger,
            minimium_diagnostics::Level::Error,
            "server.shutdown_snapshot_failed",
            &[("error_kind", format!("{:?}", error.kind()))],
        );
    }
    if snapshot.is_ok() && Instant::now() >= deadline {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "final snapshot capture exceeded shutdown deadline",
        ));
    }
    snapshot
}

fn persist_final_snapshot(
    persistence: &mut PersistenceWorker,
    snapshot: io::Result<crate::storage::SaveSnapshot>,
    deadline: Instant,
    logger: &minimium_diagnostics::DomainLogger,
) -> io::Result<()> {
    match snapshot {
        Ok(snapshot) => persistence
            .shutdown_until(Some(snapshot), deadline)
            .map_err(io::Error::from),
        Err(error) => {
            if let Err(cleanup_error) = persistence.shutdown_until(None, deadline) {
                crate::diagnostics::emit(
                    logger,
                    minimium_diagnostics::Level::Error,
                    "server.shutdown_storage_failed",
                    &[("error_code", cleanup_error.code().to_owned())],
                );
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
    logger: &minimium_diagnostics::DomainLogger,
) -> io::Result<()> {
    let endpoint_result = endpoint.remove_if_owned(data_dir);
    let mut primary_error = runtime_error;
    for (operation, result) in [
        ("final save", save_result),
        ("endpoint cleanup", endpoint_result),
    ] {
        if let Err(error) = result {
            if primary_error.is_some() {
                crate::diagnostics::emit(
                    logger,
                    minimium_diagnostics::Level::Error,
                    "server.shutdown_secondary_failure",
                    &[
                        ("operation", operation.to_owned()),
                        ("error_kind", format!("{:?}", error.kind())),
                    ],
                );
            } else {
                primary_error = Some(error);
            }
        }
    }
    let shutdown_result = primary_error.map_or(Ok(()), Err);
    let status_result = write_shutdown_status(data_dir, endpoint, &shutdown_result);
    if let Err(error) = &status_result {
        if shutdown_result.is_err() {
            crate::diagnostics::emit(
                logger,
                minimium_diagnostics::Level::Error,
                "server.shutdown_status_failed",
                &[("error_kind", format!("{:?}", error.kind()))],
            );
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
    pub(super) logger: minimium_diagnostics::DomainLogger,
}
impl Drop for EndpointGuard {
    fn drop(&mut self) {
        if let Err(error) = self.endpoint.remove_if_owned(&self.data_dir) {
            crate::diagnostics::emit(
                &self.logger,
                minimium_diagnostics::Level::Error,
                "server.endpoint_cleanup_failed",
                &[("error_kind", format!("{:?}", error.kind()))],
            );
        }
    }
}
