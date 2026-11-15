mod diagnostics;
mod launch_config;
mod process;
use diagnostics::ServerStderr;
pub use launch_config::LocalServerLaunchConfig;
use launch_config::ResolvedLaunch;
use minimium_platform::{
    admin_client,
    endpoint::{new_admin_token, ServerEndpoint},
    server_lock,
};
use process::{server_executable_path, start_local_server, terminate_and_reap, wait_for_child};
use std::{
    error::Error,
    io,
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, ExitStatus},
    thread,
    time::{Duration, Instant},
};

// Initial Earth package preparation can exceed five seconds on a cold cache.
const STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
const CHILD_EXIT_GRACE: Duration = Duration::from_secs(2);
const RETRY_INTERVAL: Duration = Duration::from_millis(25);

/// Describes whether the client attached to another server or owns a child process.
pub enum LocalServer {
    Attached,
    Owned(OwnedServer),
}

/// RAII guard for a locally started server process.
pub struct OwnedServer {
    child: Option<Child>,
    stderr: Option<ServerStderr>,
    endpoint: Option<ServerEndpoint>,
    data_dir: PathBuf,
    shutdown_attempted: bool,
    diagnostic_logger: Option<minimium_diagnostics::DomainLogger>,
}

impl LocalServer {
    /// Returns the private admin endpoint only while the owned child is alive.
    pub fn owned_admin_endpoint(&mut self) -> io::Result<Option<ServerEndpoint>> {
        match self {
            Self::Attached => Ok(None),
            Self::Owned(server) => {
                if server.is_running()? {
                    Ok(server.endpoint.clone())
                } else {
                    Ok(None)
                }
            }
        }
    }

    /// Reports whether this guard still owns a running child process.
    pub fn is_owned_process_running(&mut self) -> io::Result<bool> {
        match self {
            Self::Attached => Ok(false),
            Self::Owned(server) => server.is_running(),
        }
    }

    /// Gracefully stops an owned server and reaps its child process.
    pub fn shutdown(&mut self) -> io::Result<()> {
        match self {
            Self::Attached => Ok(()),
            Self::Owned(server) => server.shutdown(),
        }
    }
}

impl OwnedServer {
    fn is_running(&mut self) -> io::Result<bool> {
        match self.child.as_mut() {
            Some(child) => Ok(child.try_wait()?.is_none()),
            None => Ok(false),
        }
    }

    fn shutdown(&mut self) -> io::Result<()> {
        if self.shutdown_attempted {
            if let Some(child) = self.child.as_mut() {
                terminate_and_reap(child)?;
                self.child = None;
            }
            return Ok(());
        }
        self.shutdown_attempted = true;
        let status = match self.child.as_mut().map(Child::try_wait).transpose() {
            Ok(status) => status,
            Err(error) => {
                return match self.child.as_mut() {
                    Some(child) => match terminate_and_reap(child) {
                        Ok(()) => {
                            self.child = None;
                            Err(error)
                        }
                        Err(cleanup_error) => Err(io::Error::other(format!(
                            "checking owned server failed ({error}); cleanup failed ({cleanup_error})"
                        ))),
                    },
                    None => Err(error),
                };
            }
        };
        let Some(status) = status else {
            return Ok(());
        };
        if let Some(status) = status {
            self.child = None;
            return match &self.endpoint {
                Some(endpoint) if admin_client::shutdown_completed(&self.data_dir, endpoint)? => {
                    Ok(())
                }
                _ => Err(io::Error::other(format!(
                    "owned server exited before shutdown completed: {status}"
                ))),
            };
        }
        let child = self
            .child
            .as_mut()
            .expect("child remains present until reaped");
        let graceful = match &self.endpoint {
            Some(endpoint) => admin_client::stop_and_wait(&self.data_dir, endpoint),
            None => Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "owned server did not publish its endpoint",
            )),
        };
        if let Err(error) = graceful {
            return match terminate_and_reap(child) {
                Ok(()) => {
                    self.child = None;
                    Err(error)
                }
                Err(cleanup_error) => Err(io::Error::other(format!(
                    "graceful shutdown failed ({error}); cleanup failed ({cleanup_error})"
                ))),
            };
        }
        match wait_for_child(child, CHILD_EXIT_GRACE) {
            Ok(()) => {
                self.child = None;
                Ok(())
            }
            Err(error) => match terminate_and_reap(child) {
                Ok(()) => {
                    self.child = None;
                    Err(error)
                }
                Err(cleanup_error) => Err(io::Error::other(format!(
                    "waiting for owned server failed ({error}); cleanup failed ({cleanup_error})"
                ))),
            },
        }
    }

    fn terminate(&mut self) -> io::Result<()> {
        self.shutdown_attempted = true;
        if let Some(child) = self.child.as_mut() {
            terminate_and_reap(child)?;
            self.child = None;
        }
        self.endpoint = None;
        Ok(())
    }
}

impl Drop for OwnedServer {
    fn drop(&mut self) {
        if self.child.is_some() {
            if let Err(error) = self.shutdown() {
                crate::diagnostics::emit(
                    self.diagnostic_logger.as_ref(),
                    minimium_diagnostics::Level::Error,
                    "client_runtime.owned_server_shutdown_failed",
                    [crate::diagnostics::field("reason", error)],
                );
                if let Some(child) = self.child.as_mut() {
                    if let Err(cleanup_error) = terminate_and_reap(child) {
                        crate::diagnostics::emit(
                            self.diagnostic_logger.as_ref(),
                            minimium_diagnostics::Level::Error,
                            "client_runtime.owned_server_final_cleanup_failed",
                            [crate::diagnostics::field("reason", cleanup_error)],
                        );
                    } else {
                        self.child = None;
                    }
                }
            }
        }
    }
}

/// Connects to a validated local server or starts and owns one.
/// The returned game stream has not sent an identity and remains blocking.
/// Starts or attaches without exceeding the caller's connection deadline.
pub fn connect_or_start_until(
    config: &LocalServerLaunchConfig,
    deadline: Instant,
) -> Result<(TcpStream, LocalServer), Box<dyn Error>> {
    let deadline = deadline.min(Instant::now() + STARTUP_TIMEOUT);
    let executable = server_executable_path()?;
    let resolved = config.resolve(&executable, deadline)?;
    let data_dir = &resolved.data_dir;

    let existing_endpoint = match ServerEndpoint::read(data_dir) {
        Ok(endpoint) => Some(endpoint),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(endpoint) = existing_endpoint {
        if endpoint_is_live(data_dir, &endpoint, deadline)? {
            reject_unapplied_settings(config, &resolved)?;
            let stream = connect_game(&endpoint, remaining(deadline)?)?;
            return Ok((stream, LocalServer::Attached));
        }
    }

    remaining(deadline)?;
    let expected_token = new_admin_token()?;
    let attempt_id = minimium_platform::startup_status::new_attempt_id()?;
    let child = start_local_server(config, &resolved, &executable, &expected_token, &attempt_id)?;
    // This guard is established immediately after spawn, before any fallible startup work.
    let mut owned = OwnedServer {
        child: Some(child),
        stderr: None,
        endpoint: None,
        data_dir: data_dir.to_owned(),
        shutdown_attempted: false,
        diagnostic_logger: config.diagnostic_logger.clone(),
    };
    owned.stderr = Some(ServerStderr::capture(
        owned.child.as_mut().expect("startup child exists"),
    )?);
    let mut child_exit_status: Option<ExitStatus> = None;
    loop {
        if child_exit_status.is_none() {
            let status = match owned.child.as_mut().map(Child::try_wait).transpose() {
                Ok(status) => status.flatten(),
                Err(error) => {
                    if let Some(child) = owned.child.as_mut() {
                        owned.shutdown_attempted = true;
                        match terminate_and_reap(child) {
                            Ok(()) => owned.child = None,
                            Err(cleanup_error) => {
                                return Err(io::Error::other(format!(
                                    "checking local server startup failed ({error}); cleanup failed ({cleanup_error})"
                                )).into());
                            }
                        }
                    }
                    return Err(error.into());
                }
            };
            if let Some(status) = status {
                owned.child = None;
                child_exit_status = Some(status);
            }
        }

        let endpoint = match ServerEndpoint::read(data_dir) {
            Ok(endpoint) => Some(endpoint),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if let Some(endpoint) = endpoint {
            let attempt_status = read_attempt_status(data_dir, &attempt_id)?;
            if endpoint.token == expected_token
                && child_exit_status.is_none()
                && attempt_status.as_ref().is_some_and(|status| {
                    status.state == minimium_platform::startup_status::StartupState::Ready
                })
            {
                if endpoint_is_live(data_dir, &endpoint, deadline)? {
                    let status = match owned.child.as_mut().map(Child::try_wait).transpose() {
                        Ok(status) => status.flatten(),
                        Err(error) => {
                            if let Some(child) = owned.child.as_mut() {
                                owned.shutdown_attempted = true;
                                match terminate_and_reap(child) {
                                    Ok(()) => owned.child = None,
                                    Err(cleanup_error) => {
                                        return Err(io::Error::other(format!(
                                            "checking local server startup failed ({error}); cleanup failed ({cleanup_error})"
                                        )).into());
                                    }
                                }
                            }
                            return Err(error.into());
                        }
                    };
                    if let Some(status) = status {
                        owned.child = None;
                        child_exit_status = Some(status);
                        continue;
                    }
                    owned.endpoint = Some(endpoint.clone());
                    let stream = connect_game(&endpoint, remaining(deadline)?)?;
                    return Ok((stream, LocalServer::Owned(owned)));
                }
            } else if endpoint.token != expected_token
                && endpoint_is_live(data_dir, &endpoint, deadline)?
            {
                // Another process won the lock. Reap only the unsuccessful child we spawned.
                owned.terminate()?;
                reject_unapplied_settings(config, &resolved)?;
                let stream = connect_game(&endpoint, remaining(deadline)?)?;
                return Ok((stream, LocalServer::Attached));
            }
        }
        if Instant::now() >= deadline {
            if let Some(status) = child_exit_status {
                let startup = read_attempt_status(data_dir, &attempt_id)?;
                if let Some(startup) = startup.as_ref().filter(|startup| {
                    startup.state == minimium_platform::startup_status::StartupState::Failed
                }) {
                    return Err(format!(
                        "local server startup failed: {}",
                        startup
                            .error_code
                            .as_deref()
                            .unwrap_or("server.startup_failed")
                    )
                    .into());
                }
                let allow_stderr_fallback = startup.as_ref().is_none_or(|startup| {
                    startup.state == minimium_platform::startup_status::StartupState::Starting
                });
                let detail = allow_stderr_fallback
                    .then(|| owned.stderr.as_mut().and_then(ServerStderr::finish))
                    .flatten();
                return Err(match detail {
                    Some(detail) => {
                        format!("local server exited during startup ({status}): {detail}")
                    }
                    None => format!("local server exited during startup: {status}"),
                }
                .into());
            }
            // Startup never established ownership of a validated endpoint.
            // Reap only our own child and report the useful startup error rather
            // than attempting an admin shutdown against an unverified server.
            owned.terminate()?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "timed out waiting for a validated local server; rebuild both client and server with `cargo build --workspace` and check the server output",
            )
            .into());
        }
        thread::sleep(RETRY_INTERVAL);
    }
}

fn validate_live_endpoint(
    data_dir: &Path,
    endpoint: &ServerEndpoint,
    deadline: Instant,
) -> io::Result<bool> {
    admin_client::request_until(endpoint, "status", deadline)?;
    if !server_lock::is_locked(data_dir)? {
        return Ok(false);
    }
    match ServerEndpoint::read(data_dir) {
        Ok(current) => Ok(same_endpoint(&current, endpoint)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn endpoint_is_live(
    data_dir: &Path,
    endpoint: &ServerEndpoint,
    deadline: Instant,
) -> io::Result<bool> {
    match validate_live_endpoint(data_dir, endpoint, deadline) {
        Ok(is_live) => Ok(is_live),
        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => Ok(false),
        Err(error) => Err(error),
    }
}

fn read_attempt_status(
    data_dir: &Path,
    attempt_id: &str,
) -> io::Result<Option<minimium_platform::startup_status::ServerStartupStatus>> {
    match minimium_platform::startup_status::read(data_dir, attempt_id) {
        Ok(status) if status.attempt_id == attempt_id => Ok(Some(status)),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "startup status attempt identifier does not match the active attempt",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn reject_unapplied_settings(
    config: &LocalServerLaunchConfig,
    resolved: &ResolvedLaunch,
) -> io::Result<()> {
    let unapplied = config.unapplied_on_attach(resolved);
    if unapplied.is_empty() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "cannot attach to the existing local server; startup settings were not applied: {}",
            unapplied.join(", ")
        )))
    }
}

fn same_endpoint(left: &ServerEndpoint, right: &ServerEndpoint) -> bool {
    left.game_port == right.game_port
        && left.admin_port == right.admin_port
        && left.token == right.token
}

fn connect_game(endpoint: &ServerEndpoint, timeout: Duration) -> io::Result<TcpStream> {
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], endpoint.game_port));
    TcpStream::connect_timeout(&address, timeout.min(CONNECT_TIMEOUT))
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    let duration = deadline.saturating_duration_since(Instant::now());
    if duration.is_zero() {
        Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "local server startup timed out",
        ))
    } else {
        Ok(duration)
    }
}
