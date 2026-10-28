use crate::options::ThreadLane;
use minimium_core::{
    admin_client,
    endpoint::{new_admin_token, ServerEndpoint},
    server_lock,
};
use std::{
    error::Error,
    io,
    net::TcpStream,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
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
    endpoint: Option<ServerEndpoint>,
    data_dir: PathBuf,
    shutdown_attempted: bool,
}

impl LocalServer {
    /// Reports whether this guard still owns a running child process.
    pub fn is_owned_process_running(&mut self) -> io::Result<bool> {
        match self {
            Self::Attached => Ok(false),
            Self::Owned(server) => server.is_running(),
        }
    }

    /// Gracefully stops an owned server and reaps its child process.
    pub fn shutdown(&mut self) -> Result<(), Box<dyn Error>> {
        match self {
            Self::Attached => Ok(()),
            Self::Owned(server) => server.shutdown().map_err(Into::into),
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
                eprintln!("owned local server shutdown failed during drop: {error}");
                if let Some(child) = self.child.as_mut() {
                    if let Err(cleanup_error) = terminate_and_reap(child) {
                        eprintln!("owned local server final cleanup failed: {cleanup_error}");
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
    data_dir: &Path,
    map_path: Option<&Path>,
    threads: ThreadLane,
    deadline: Instant,
) -> Result<(TcpStream, LocalServer), Box<dyn Error>> {
    let deadline = deadline.min(Instant::now() + STARTUP_TIMEOUT);

    if let Ok(endpoint) = ServerEndpoint::read(data_dir) {
        if validate_live_endpoint(data_dir, &endpoint, deadline).unwrap_or(false) {
            let stream = connect_game(&endpoint, remaining(deadline)?)?;
            return Ok((stream, LocalServer::Attached));
        }
    }

    remaining(deadline)?;
    let expected_token = new_admin_token()?;
    let child = start_local_server(data_dir, map_path, threads, &expected_token)?;
    // This guard is established immediately after spawn, before any fallible startup work.
    let mut owned = OwnedServer {
        child: Some(child),
        endpoint: None,
        data_dir: data_dir.to_owned(),
        shutdown_attempted: false,
    };
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

        if let Ok(endpoint) = ServerEndpoint::read(data_dir) {
            if endpoint.token == expected_token && child_exit_status.is_none() {
                if validate_live_endpoint(data_dir, &endpoint, deadline).unwrap_or(false) {
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
                && validate_live_endpoint(data_dir, &endpoint, deadline).unwrap_or(false)
            {
                // Another process won the lock. Reap only the unsuccessful child we spawned.
                owned.terminate()?;
                let stream = connect_game(&endpoint, remaining(deadline)?)?;
                return Ok((stream, LocalServer::Attached));
            }
        }
        if Instant::now() >= deadline {
            if let Some(status) = child_exit_status {
                return Err(format!("local server exited during startup: {status}").into());
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

fn start_local_server(
    data_dir: &Path,
    map_path: Option<&Path>,
    threads: ThreadLane,
    token: &str,
) -> io::Result<Child> {
    let current = std::env::current_exe()?;
    let executable = if cfg!(windows) {
        "minimium-server.exe"
    } else {
        "minimium-server"
    };
    let path = current.with_file_name(executable);
    if !path.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("server executable not found at {}", path.display()),
        ));
    }
    let mut command = Command::new(path);
    command
        .arg("run")
        .arg("--headless")
        .arg("--threads")
        .arg(threads.as_str())
        .arg("--bind")
        .arg("127.0.0.1")
        .arg("--data-dir")
        .arg(data_dir)
        .env("MINIMIUM_ADMIN_TOKEN", token);
    if let Some(map_path) = map_path {
        command.arg("--map").arg(map_path);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "local server did not exit after graceful shutdown",
            ));
        }
        thread::sleep(RETRY_INTERVAL);
    }
}

fn terminate_and_reap(child: &mut Child) -> io::Result<()> {
    if let Err(kill_error) = child.kill() {
        // An already-exited process is still reaped. Do not wait indefinitely
        // on a live process when the OS refused the termination request.
        return match child.try_wait() {
            Ok(Some(_)) => child.wait().map(|_| ()),
            Ok(None) => Err(kill_error),
            Err(wait_error) => Err(io::Error::other(format!(
                "killing child failed ({kill_error}); checking child failed ({wait_error})"
            ))),
        };
    }
    child.wait().map(|_| ())
}
