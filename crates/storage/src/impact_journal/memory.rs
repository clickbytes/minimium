use super::*;
use crate::memory::Lease;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

pub(crate) struct MemoryImpacts {
    lease: Arc<Lease>,
    maximum: usize,
    pending: Mutex<VecDeque<Vec<BodyState>>>,
    stopped: bool,
}
impl MemoryImpacts {
    pub(crate) fn new(lease: Arc<Lease>, maximum: usize) -> Self {
        Self {
            lease,
            maximum,
            pending: Mutex::new(VecDeque::new()),
            stopped: false,
        }
    }
}
impl ImpactJournalPort for MemoryImpacts {
    fn submit_batch(&self, batch: Vec<BodyState>) -> io::Result<()> {
        if self.stopped {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "memory impacts stopped",
            ));
        }
        if batch.is_empty() || batch.len() > 2 || batch.iter().any(|body| !body.valid()) {
            return Err(invalid("invalid body state"));
        }
        let mut queue = self
            .pending
            .lock()
            .map_err(|_| invalid("memory impacts poisoned"))?;
        if queue.len() >= 16 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "memory impacts queue full",
            ));
        }
        queue.push_back(batch);
        Ok(())
    }
    fn poll(&self) -> Option<io::Result<Vec<BodyState>>> {
        let batch = match self.pending.lock() {
            Ok(mut pending) => pending.pop_front()?,
            Err(_) => return Some(Err(invalid("memory impacts poisoned"))),
        };
        Some((|| {
            let mut state = self
                .lease
                .state
                .lock()
                .map_err(|_| invalid("memory backend poisoned"))?;
            let mut staged = state.impacts.clone();
            for body in &batch {
                insert(&mut staged, *body)?;
            }
            if staged.len() > self.maximum {
                return Err(io::Error::new(
                    io::ErrorKind::FileTooLarge,
                    "memory impact capacity exceeded",
                ));
            }
            state.impacts = staged;
            Ok(batch)
        })())
    }
    fn shutdown_until(&mut self, deadline: Instant) -> io::Result<()> {
        while !self
            .pending
            .lock()
            .map_err(|_| invalid("memory impacts poisoned"))?
            .is_empty()
        {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "memory impacts shutdown timeout",
                ));
            }
            if let Some(result) = self.poll() {
                result?;
            }
        }
        self.stopped = true;
        Ok(())
    }
}
