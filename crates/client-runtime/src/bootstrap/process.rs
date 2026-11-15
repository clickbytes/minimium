use super::{LocalServerLaunchConfig, ResolvedLaunch, RETRY_INTERVAL};
use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

pub(super) fn server_executable_path() -> io::Result<PathBuf> {
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
    Ok(path)
}

pub(super) fn start_local_server(
    config: &LocalServerLaunchConfig,
    resolved: &ResolvedLaunch,
    executable: &Path,
    token: &str,
    attempt_id: &str,
) -> io::Result<Child> {
    let mut command = Command::new(executable);
    config.prepare_command(&mut command);
    config.append_run_arguments(&mut command, resolved, attempt_id);
    command.env("MINIMIUM_ADMIN_TOKEN", token);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
}

pub(super) fn wait_for_child(child: &mut Child, timeout: Duration) -> io::Result<()> {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "local server did not exit before the child deadline",
            ));
        }
        thread::sleep(RETRY_INTERVAL);
    }
}

pub(super) fn terminate_and_reap(child: &mut Child) -> io::Result<()> {
    if let Err(kill_error) = child.kill() {
        // An already-exited process is still reaped. Do not wait indefinitely
        // on a live process when the OS refused the termination request.
        return match child.try_wait() {
            Ok(Some(_)) => Ok(()),
            Ok(None) => Err(kill_error),
            Err(wait_error) => Err(io::Error::other(format!(
                "killing child failed ({kill_error}); checking child failed ({wait_error})"
            ))),
        };
    }
    wait_for_child(child, Duration::from_secs(2))
}
