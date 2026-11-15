//! Bounded HTTP/1.1 asset transport, separate from game traffic.
use crate::{AssetIoError, AssetIoErrorKind};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::{Duration, Instant},
};

/// Endpoint is the game peer's IP with the separately advertised asset port.
pub fn fetch(
    address: SocketAddr,
    name: &str,
    limit: u64,
    output: &mut dyn Write,
) -> Result<(), AssetIoError> {
    fetch_cancellable(address, name, limit, output, || false)
}

pub fn fetch_cancellable(
    address: SocketAddr,
    name: &str,
    limit: u64,
    output: &mut dyn Write,
    cancelled: impl Fn() -> bool,
) -> Result<(), AssetIoError> {
    let deadline = Instant::now() + Duration::from_secs(300);
    let interrupted = || cancelled() || Instant::now() >= deadline;
    if !name
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        return Err(AssetIoError::protocol(
            "material request path",
            "invalid asset name",
        ));
    }
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(10))
        .map_err(|error| AssetIoError::io("connect to material server", error))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|error| AssetIoError::io("set material read timeout", error))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| AssetIoError::io("set material write timeout", error))?;
    write!(
        stream,
        "GET /{name} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|error| AssetIoError::io("write material request", error))?;
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if interrupted() {
            return Err(interruption_error(&cancelled, deadline));
        }
        if header.len() >= 8192 {
            return Err(AssetIoError::limit_exceeded("material HTTP header", 8192));
        }
        let mut byte = [0];
        stream
            .read_exact(&mut byte)
            .map_err(|error| AssetIoError::io("read material HTTP header", error))?;
        header.push(byte[0]);
    }
    let text = std::str::from_utf8(&header).map_err(|error| {
        AssetIoError::caused_by(
            AssetIoErrorKind::Protocol,
            "material HTTP header encoding",
            error,
        )
    })?;
    let mut lines = text.lines();
    if lines.next() != Some("HTTP/1.1 200 OK") {
        return Err(AssetIoError::protocol(
            "material HTTP response",
            "unexpected status",
        ));
    }
    let mut length = None;
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("transfer-encoding") {
                return Err(AssetIoError::protocol(
                    "material HTTP response",
                    "chunked encoding is unsupported",
                ));
            }
            if key.eq_ignore_ascii_case("content-length") {
                if length.is_some() {
                    return Err(AssetIoError::protocol(
                        "material HTTP response",
                        "duplicate content length",
                    ));
                }
                length = Some(value.trim().parse::<u64>().map_err(|error| {
                    AssetIoError::caused_by(
                        AssetIoErrorKind::Protocol,
                        "material content length",
                        error,
                    )
                })?);
            }
        }
    }
    let length = length.ok_or_else(|| {
        AssetIoError::protocol("material HTTP response", "missing content length")
    })?;
    if length > limit {
        return Err(AssetIoError::limit_exceeded(
            "material response size",
            limit,
        ));
    }
    let mut remaining = length;
    let mut buffer = [0_u8; 64 * 1024];
    while remaining > 0 {
        if interrupted() {
            return Err(interruption_error(&cancelled, deadline));
        }
        let count = (remaining as usize).min(buffer.len());
        let read = stream
            .read(&mut buffer[..count])
            .map_err(|error| AssetIoError::io("read material response", error))?;
        if read == 0 {
            return Err(AssetIoError::io(
                "read material response",
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "response ended early"),
            ));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| AssetIoError::io("write material cache file", error))?;
        remaining -= read as u64;
    }
    Ok(())
}

fn interruption_error(cancelled: &impl Fn() -> bool, deadline: Instant) -> AssetIoError {
    if cancelled() {
        AssetIoError::cancelled("material download")
    } else if Instant::now() >= deadline {
        AssetIoError::timeout("material download deadline")
    } else {
        AssetIoError::timeout("material download interrupted")
    }
}
