use std::{
    io::{self, Write},
    net::TcpStream,
};

/// Result of flushing the queued bytes for one client.
pub struct FlushResult {
    pub written: usize,
    pub error: Option<io::Error>,
}

/// Writes queued output while retaining all bytes not yet sent.
pub fn flush(stream: &mut TcpStream, output: &[u8], offset: &mut usize) -> FlushResult {
    let mut written = 0;
    if *offset > output.len() {
        return FlushResult {
            written,
            error: Some(io::Error::new(
                io::ErrorKind::InvalidInput,
                "output offset exceeds queued bytes",
            )),
        };
    }

    while *offset < output.len() {
        match stream.write(&output[*offset..]) {
            Ok(0) => {
                return FlushResult {
                    written,
                    error: Some(io::Error::new(
                        io::ErrorKind::WriteZero,
                        "failed to write queued client output",
                    )),
                };
            }
            Ok(count) => {
                *offset += count;
                written += count;
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                return FlushResult {
                    written,
                    error: None,
                };
            }
            Err(error) => {
                return FlushResult {
                    written,
                    error: Some(error),
                };
            }
        }
    }

    FlushResult {
        written,
        error: None,
    }
}
