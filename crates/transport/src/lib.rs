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
pub fn flush(
    stream: &mut TcpStream,
    output: &[u8],
    offset: &mut usize,
    budget: usize,
) -> FlushResult {
    flush_writer(stream, output, offset, budget)
}

fn flush_writer(
    writer: &mut impl Write,
    output: &[u8],
    offset: &mut usize,
    budget: usize,
) -> FlushResult {
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

    while *offset < output.len() && written < budget {
        let end = output.len().min(offset.saturating_add(budget - written));
        match writer.write(&output[*offset..end]) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    enum WriteStep {
        Bytes(usize),
        Error(io::ErrorKind),
    }

    struct ScriptedWriter {
        steps: VecDeque<WriteStep>,
        captured: Vec<u8>,
    }

    impl Write for ScriptedWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            match self
                .steps
                .pop_front()
                .unwrap_or(WriteStep::Bytes(buffer.len()))
            {
                WriteStep::Bytes(count) => {
                    let count = count.min(buffer.len());
                    self.captured.extend_from_slice(&buffer[..count]);
                    Ok(count)
                }
                WriteStep::Error(kind) => Err(io::Error::new(kind, "scripted write result")),
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn zero_byte_write_returns_write_zero_after_accounting_partial_bytes() {
        let mut writer = ScriptedWriter {
            steps: VecDeque::from([WriteStep::Bytes(2), WriteStep::Bytes(0)]),
            captured: Vec::new(),
        };
        let output = b"abcdef";
        let mut offset = 0;

        let result = flush_writer(&mut writer, output, &mut offset, output.len());

        assert_eq!(result.written, 2);
        assert_eq!(offset, 2);
        assert_eq!(
            result.error.map(|error| error.kind()),
            Some(io::ErrorKind::WriteZero)
        );
        assert_eq!(writer.captured, b"ab");
    }

    #[test]
    fn partial_write_survives_would_block_and_resumes_at_offset() {
        let mut writer = ScriptedWriter {
            steps: VecDeque::from([
                WriteStep::Bytes(2),
                WriteStep::Error(io::ErrorKind::WouldBlock),
            ]),
            captured: Vec::new(),
        };
        let output = b"abcdef";
        let mut offset = 0;

        let first = flush_writer(&mut writer, output, &mut offset, output.len());
        assert_eq!(first.written, 2);
        assert!(first.error.is_none());
        assert_eq!(offset, 2);

        let second = flush_writer(&mut writer, output, &mut offset, output.len());
        assert_eq!(second.written, 4);
        assert!(second.error.is_none());
        assert_eq!(offset, output.len());
        assert_eq!(writer.captured, output);
    }
}
