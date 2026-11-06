//! Bounded HTTP/1.1 asset transport, separate from game traffic.
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
) -> Result<(), String> {
    fetch_cancellable(address, name, limit, output, || false)
}

pub fn fetch_cancellable(
    address: SocketAddr,
    name: &str,
    limit: u64,
    output: &mut dyn Write,
    cancelled: impl Fn() -> bool,
) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(300);
    let interrupted = || cancelled() || Instant::now() >= deadline;
    if !name
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    {
        return Err("invalid material HTTP path".into());
    }
    let mut stream =
        TcpStream::connect_timeout(&address, Duration::from_secs(10)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    write!(
        stream,
        "GET /{name} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )
    .map_err(|e| e.to_string())?;
    let mut header = Vec::new();
    while !header.ends_with(b"\r\n\r\n") {
        if interrupted() {
            return Err("material download cancelled or timed out".into());
        }
        if header.len() >= 8192 {
            return Err("HTTP header exceeds limit".into());
        }
        let mut byte = [0];
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        header.push(byte[0]);
    }
    let text = std::str::from_utf8(&header).map_err(|e| e.to_string())?;
    let mut lines = text.lines();
    if lines.next() != Some("HTTP/1.1 200 OK") {
        return Err("material HTTP request failed".into());
    }
    let mut length = None;
    for line in lines {
        if let Some((key, value)) = line.split_once(':') {
            if key.eq_ignore_ascii_case("transfer-encoding") {
                return Err("chunked material response unsupported".into());
            }
            if key.eq_ignore_ascii_case("content-length") {
                if length.is_some() {
                    return Err("duplicate content-length".into());
                }
                length = Some(value.trim().parse::<u64>().map_err(|e| e.to_string())?);
            }
        }
    }
    let length = length
        .filter(|v| *v <= limit)
        .ok_or("invalid material content-length")?;
    let mut remaining = length;
    let mut buffer = [0_u8; 64 * 1024];
    while remaining > 0 {
        if interrupted() {
            return Err("material download cancelled or timed out".into());
        }
        let count = (remaining as usize).min(buffer.len());
        let read = stream
            .read(&mut buffer[..count])
            .map_err(|e| e.to_string())?;
        if read == 0 {
            return Err("material download interrupted".into());
        }
        output
            .write_all(&buffer[..read])
            .map_err(|e| e.to_string())?;
        remaining -= read as u64;
    }
    Ok(())
}
