use super::*;
use crate::memory::Lease;
use std::collections::VecDeque;

pub(crate) struct MemoryExcavation {
    lease: Arc<Lease>,
    pending: Mutex<VecDeque<WorldMutation>>,
    config: JournalConfig,
    stopped: bool,
}

impl MemoryExcavation {
    pub(crate) fn new(lease: Arc<Lease>, config: JournalConfig) -> Self {
        Self {
            lease,
            pending: Mutex::new(VecDeque::new()),
            config,
            stopped: false,
        }
    }
    fn check(&self, mutation: &WorldMutation, state: &ExcavationRestore) -> io::Result<()> {
        if mutation.expected_chunk_revisions.len() > 64
            || mutation.craters.len() > minimium_world::terrain_edits::MAX_TERRAIN_EDITS
        {
            return Err(invalid_request("excavation mutation count exceeds limit"));
        }
        validate_mutation(mutation, state)?;
        // Reject before the encoder allocates: a smaller host transaction cap
        // must also bound the staging image, not just accepted queue entries.
        let mut bytes = 51usize;
        bytes = bytes.saturating_add(mutation.expected_chunk_revisions.len() * 20);
        bytes =
            bytes.saturating_add(mutation.resulting_chunks.len() * (20 + SAMPLES_PER_CHUNK * 9));
        bytes = bytes.saturating_add(
            mutation.impact_bodies.len() * minimium_world::impact::BODY_RECORD_BYTES,
        );
        bytes = bytes.saturating_add(mutation.craters.len() * 56);
        bytes = bytes.saturating_add(usize::from(mutation.inventory.is_some()) * 27);
        bytes = bytes.saturating_add(usize::from(mutation.water_catalog_digest.is_some()) * 32);
        if let Some(fluid) = &mutation.fluids {
            bytes = bytes.saturating_add(budget::fluid_size(fluid)?);
        }
        if bytes > self.config.transaction_budget {
            return Err(invalid_request(
                "excavation transaction byte budget exceeded",
            ));
        }
        if encode_mutation(mutation)?.len() > self.config.transaction_budget {
            return Err(invalid_request(
                "excavation transaction byte budget exceeded",
            ));
        }
        let mut candidate = state.clone();
        apply(&mut candidate, mutation);
        restore_validation::checked_checkpoint(&candidate, self.config.byte_budget as usize)?;
        Ok(())
    }
}

impl ExcavationJournalPort for MemoryExcavation {
    fn submit(&self, mutation: WorldMutation) -> io::Result<()> {
        self.validate(&mutation)?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| invalid("memory journal poisoned"))?;
        if pending.len() >= QUEUE_CAPACITY {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "excavation queue full",
            ));
        }
        pending.push_back(mutation);
        Ok(())
    }
    fn validate(&self, mutation: &WorldMutation) -> io::Result<()> {
        if self.stopped {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "excavation journal stopped",
            ));
        }
        let state = self
            .lease
            .state
            .lock()
            .map_err(|_| invalid("memory backend poisoned"))?;
        self.check(mutation, &state.excavation)
    }
    fn poll_durable(&self) -> Option<io::Result<WorldMutation>> {
        let mutation = match self.pending.lock() {
            Ok(mut pending) => pending.pop_front()?,
            Err(_) => return Some(Err(invalid("memory journal poisoned"))),
        };
        Some((|| {
            let mut state = self
                .lease
                .state
                .lock()
                .map_err(|_| invalid("memory backend poisoned"))?;
            self.check(&mutation, &state.excavation)?;
            apply(&mut state.excavation, &mutation);
            Ok(mutation)
        })())
    }
    fn counters(&self) -> (u64, u64) {
        let state = self
            .lease
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        (
            state.excavation.mutation_count,
            state.excavation.last_mutation_id,
        )
    }
    fn journal_bytes(&self) -> io::Result<u64> {
        let state = self
            .lease
            .state
            .lock()
            .map_err(|_| invalid("memory backend poisoned"))?;
        Ok(restore_validation::checked_checkpoint(
            &state.excavation,
            self.config.byte_budget as usize,
        )?
        .len() as u64)
    }
    fn shutdown_until(&mut self, deadline: Instant) -> io::Result<()> {
        while !self
            .pending
            .lock()
            .map_err(|_| invalid("memory journal poisoned"))?
            .is_empty()
        {
            if Instant::now() >= deadline {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "memory journal shutdown timeout",
                ));
            }
            if let Some(result) = self.poll_durable() {
                result?;
            }
        }
        self.stopped = true;
        Ok(())
    }
}
