use crate::{endpoint::ServerEndpoint, server_lock::ServerDataLock};
use std::{
    io::{self, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    thread,
    time::{Duration, Instant},
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(3);
const STOP_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// Sends one authenticated administrative command and returns the complete response.
pub fn request(endpoint: &ServerEndpoint, command: &str) -> io::Result<String> {
    request_until(
        endpoint,
        command,
        Instant::now() + CONNECT_TIMEOUT + RESPONSE_TIMEOUT,
    )
}

/// Sends an authenticated command without exceeding the caller's overall deadline.
pub fn request_until(
    endpoint: &ServerEndpoint,
    command: &str,
    deadline: Instant,
) -> io::Result<String> {
    if command.len() > 512
        || endpoint.token.len() != 64
        || !endpoint.token.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid admin request",
        ));
    }
    crate::commands::ServerCommand::parse(command)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let connect_remaining = deadline.saturating_duration_since(Instant::now());
    if connect_remaining.is_zero() {
        return Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "admin request deadline expired",
        ));
    }
    let address = ("127.0.0.1", endpoint.admin_port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::AddrNotAvailable,
                "admin endpoint unavailable",
            )
        })?;
    let mut stream = TcpStream::connect_timeout(&address, CONNECT_TIMEOUT.min(connect_remaining))?;
    let response_deadline =
        Instant::now() + RESPONSE_TIMEOUT.min(deadline.saturating_duration_since(Instant::now()));
    let request = format!("AUTH {}\n{}\n", endpoint.token, command);
    write_before_deadline(&mut stream, request.as_bytes(), response_deadline)?;

    let mut response = Vec::with_capacity(1024);
    let mut chunk = [0u8; 4096];
    loop {
        let remaining = response_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "admin response timed out",
            ));
        }
        stream.set_read_timeout(Some(remaining))?;
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                if response.len() + count > MAX_RESPONSE_BYTES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "admin response exceeds 64 KiB",
                    ));
                }
                response.extend_from_slice(&chunk[..count]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    let response = String::from_utf8(response)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "admin response is not UTF-8"))?;
    let response = response.trim();
    if let Some(payload) = response.strip_prefix("OK ") {
        return Ok(payload.to_owned());
    }
    if response == "OK" {
        return Ok(String::new());
    }
    if let Some(message) = response.strip_prefix("ERR ") {
        return Err(io::Error::other(message.to_owned()));
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "server returned an invalid response",
    ))
}

/// Confirms a successful shutdown record for this endpoint while holding the released data lock.
pub fn shutdown_completed(data_dir: &Path, endpoint: &ServerEndpoint) -> io::Result<bool> {
    if !data_dir.is_dir() {
        return Ok(false);
    }
    let status_path = data_dir.join("server.shutdown");
    let Some(status) = read_shutdown_status(&status_path)? else {
        return Ok(false);
    };
    if !successful_shutdown_status(&status, endpoint.admin_port) {
        return Ok(false);
    }
    let lock = match ServerDataLock::acquire(data_dir) {
        Ok(lock) => lock,
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
        Err(error) => return Err(error),
    };
    let owner = read_shutdown_status(&data_dir.join("server.shutdown.owner"))?;
    if owner.as_deref().map(str::trim_end) != Some(endpoint.token.as_str()) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "shutdown result belongs to another instance",
        ));
    }
    let confirmed_status = read_shutdown_status(&status_path)?;
    if confirmed_status.as_deref() != Some(status.as_str()) {
        drop(lock);
        return Ok(false);
    }
    match ServerEndpoint::read(data_dir) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Ok(_) => {
            drop(lock);
            return Ok(false);
        }
        Err(error) => {
            drop(lock);
            return Err(error);
        }
    }
    drop(lock);
    Ok(true)
}

fn write_before_deadline(
    stream: &mut TcpStream,
    mut bytes: &[u8],
    deadline: Instant,
) -> io::Result<()> {
    while !bytes.is_empty() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "admin request timed out",
            ));
        }
        stream.set_write_timeout(Some(remaining))?;
        match stream.write(bytes) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "admin request write failed",
                ))
            }
            Ok(count) => bytes = &bytes[count..],
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    stream.flush()
}

/// Waits for the captured server instance to report successful shutdown and release its data lock.
pub fn stop_and_wait(data_dir: &Path, endpoint: &ServerEndpoint) -> io::Result<()> {
    let deadline = Instant::now() + STOP_TIMEOUT;
    let status_path = data_dir.join("server.shutdown");
    if !data_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "server data directory disappeared during shutdown",
        ));
    }
    ensure_endpoint_matches(data_dir, endpoint)?;
    if request(endpoint, "stop")? != "stopping" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "server returned an unexpected stop response",
        ));
    }
    loop {
        ensure_endpoint_matches_or_absent(data_dir, endpoint)?;
        let status = read_shutdown_status(&status_path)?;
        if let Some(status) = status {
            let mut fields = status.trim_end().split('\t');
            let expected_port = endpoint.admin_port.to_string();
            if fields.next() == Some(expected_port.as_str()) {
                match fields.next() {
                    Some("OK") if fields.next().is_none() => {
                        match ServerDataLock::acquire(data_dir) {
                            Ok(lock) => {
                                let owner =
                                    read_shutdown_status(&data_dir.join("server.shutdown.owner"))?;
                                if owner.as_deref().map(str::trim_end)
                                    != Some(endpoint.token.as_str())
                                {
                                    return Err(io::Error::new(
                                        io::ErrorKind::Interrupted,
                                        "shutdown result belongs to another instance",
                                    ));
                                }
                                let confirmed_status = read_shutdown_status(&status_path)?;
                                if confirmed_status.as_deref() != Some(status.as_str()) {
                                    drop(lock);
                                    return Err(io::Error::new(
                                        io::ErrorKind::Interrupted,
                                        "shutdown status changed during verification",
                                    ));
                                }
                                match ServerEndpoint::read(data_dir) {
                                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                                    Ok(_) => {
                                        drop(lock);
                                        return Err(io::Error::new(
                                            io::ErrorKind::Interrupted,
                                            "server endpoint still exists after shutdown",
                                        ));
                                    }
                                    Err(error) => {
                                        drop(lock);
                                        return Err(error);
                                    }
                                }
                                drop(lock);
                                return Ok(());
                            }
                            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                            Err(error) => return Err(error),
                        }
                    }
                    Some("ERR") => {
                        return Err(io::Error::other(format!(
                            "server shutdown failed: {}",
                            fields.next().unwrap_or("unknown error")
                        )))
                    }
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            "server wrote an invalid shutdown status",
                        ))
                    }
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "server did not finish shutting down within 120 seconds",
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn ensure_endpoint_matches(data_dir: &Path, expected: &ServerEndpoint) -> io::Result<()> {
    match ServerEndpoint::read(data_dir) {
        Ok(current) if same_endpoint(&current, expected) => Ok(()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "server endpoint changed before shutdown",
        )),
        Err(error) => Err(error),
    }
}

fn ensure_endpoint_matches_or_absent(data_dir: &Path, expected: &ServerEndpoint) -> io::Result<()> {
    match ServerEndpoint::read(data_dir) {
        Ok(current) if same_endpoint(&current, expected) => Ok(()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "server instance was replaced during shutdown",
        )),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn same_endpoint(left: &ServerEndpoint, right: &ServerEndpoint) -> bool {
    left.game_port == right.game_port
        && left.admin_port == right.admin_port
        && left.token == right.token
}

fn successful_shutdown_status(status: &str, admin_port: u16) -> bool {
    let mut fields = status.trim_end().split('\t');
    fields.next() == Some(admin_port.to_string().as_str())
        && fields.next() == Some("OK")
        && fields.next().is_none()
}

fn read_shutdown_status(path: &Path) -> io::Result<Option<String>> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::with_capacity(256);
    file.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "shutdown status exceeds 4096 bytes",
        ));
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "shutdown status is not UTF-8"))
}
