use crate::{
    admin::AdminServer,
    connection::Client,
    storage::{compact_error, PersistenceWorker, PlayerStore},
};
use minimium_core::{
    admin_client::{SHUTDOWN_FILE, SHUTDOWN_OWNER_FILE},
    endpoint::ServerEndpoint,
    files::atomic_replace_file,
    world::World,
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
    pub world: &'a World,
    pub map_fingerprint: u64,
    pub endpoint: &'a ServerEndpoint,
    pub data_dir: &'a Path,
    pub runtime_error: Option<io::Error>,
}

pub(crate) fn shutdown(mut context: ShutdownContext<'_>) -> io::Result<()> {
    context.admin.reject_pending("server stopping");
    let drain_deadline = Instant::now() + Duration::from_secs(2);
    while context.admin.has_pending_output() && Instant::now() < drain_deadline {
        match context.admin.poll(Instant::now()) {
            Ok(requests) => {
                for request in requests {
                    context.admin.respond(request.id, false, "server stopping");
                }
            }
            Err(error) => {
                context.runtime_error.get_or_insert(error);
                break;
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
    for client in context.clients {
        let _ = client.stream.shutdown(std::net::Shutdown::Both);
    }
    let save_result = match context
        .player_store
        .save_snapshot(context.world, context.map_fingerprint)
    {
        Ok(snapshot) => context.persistence.shutdown(Some(snapshot)),
        Err(error) => context.persistence.shutdown(None).and(Err(error)),
    };
    let endpoint_result = context.endpoint.remove_if_owned(context.data_dir);
    let shutdown_result = save_result
        .and(endpoint_result)
        .and(context.runtime_error.map_or(Ok(()), Err));
    let status_result = write_shutdown_status(context.data_dir, context.endpoint, &shutdown_result);
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
