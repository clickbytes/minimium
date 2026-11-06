//! Drain child stderr continuously, preserving a bounded diagnostic tail for UI errors.
use std::{
    collections::VecDeque,
    io::{self, Read, Write},
    process::Child,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

const MAX_TAIL_BYTES: usize = 8192;
const MAX_MESSAGE_CHARS: usize = 2048;

pub(super) struct ServerStderr {
    tail: Arc<Mutex<VecDeque<u8>>>,
    reader: Option<JoinHandle<()>>,
}

impl ServerStderr {
    /// The caller establishes its child-process guard before starting this reader.
    pub(super) fn capture(child: &mut Child) -> io::Result<Self> {
        let mut stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("local server stderr pipe is missing"))?;
        let tail = Arc::new(Mutex::new(VecDeque::with_capacity(MAX_TAIL_BYTES)));
        let output = tail.clone();
        let reader = std::thread::Builder::new()
            .name("local-server-stderr".into())
            .spawn(move || {
                let mut buffer = [0; 1024];
                loop {
                    let count = match stderr.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(count) => count,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(_) => break,
                    };
                    if let Ok(mut tail) = output.lock() {
                        let discard = (tail.len() + count).saturating_sub(MAX_TAIL_BYTES);
                        tail.drain(..discard);
                        tail.extend(&buffer[..count]);
                    }
                    // Preserve terminal diagnostics as well as the in-game error.
                    let _ = io::stderr().write_all(&buffer[..count]);
                }
            })?;
        Ok(Self {
            tail,
            reader: Some(reader),
        })
    }

    /// Call only after the owned child has exited, so its pipe can reach EOF.
    pub(super) fn finish(&mut self) -> Option<String> {
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        let bytes: Vec<u8> = self.tail.lock().ok()?.iter().copied().collect();
        let text = String::from_utf8_lossy(&bytes);
        let lines: Vec<_> = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .collect();
        let detail = lines
            .iter()
            .rev()
            .find(|line| line.starts_with("server error:"))
            .or_else(|| lines.last())?;
        let detail: String = detail
            .chars()
            .filter(|c| !c.is_control())
            .take(MAX_MESSAGE_CHARS)
            .collect();
        (!detail.is_empty()).then_some(detail)
    }
}
