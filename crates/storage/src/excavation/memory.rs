use super::*;
use crate::memory::Lease;
use minimium_gameplay::{WorldChangeAck, WorldChangeCommitment, WorldChangeFailure};
use std::collections::VecDeque;

pub(crate) struct MemoryExcavation {
    lease: Arc<Lease>,
    pending: Mutex<VecDeque<WorldChangePlan>>,
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
    fn check(
        &self,
        plan: &WorldChangePlan,
        state: &ExcavationRestore,
    ) -> Result<(), PersistenceError> {
        let mutation = stored_mutation(plan);
        if mutation.expected_chunk_revisions.len() > 64
            || mutation.craters.len() > minimium_world::terrain_edits::MAX_TERRAIN_EDITS
        {
            return Err(PersistenceError::new(WorldChangeFailure::LimitExceeded));
        }
        validate_mutation(&mutation, state).map_err(|error| {
            PersistenceError::with_source(
                error.kind,
                io::Error::new(io::ErrorKind::InvalidData, error.detail),
            )
        })?;
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
            bytes = bytes.saturating_add(budget::fluid_size(fluid).map_err(PersistenceError::new)?);
        }
        if bytes > self.config.transaction_budget {
            return Err(PersistenceError::new(WorldChangeFailure::LimitExceeded));
        }
        if encode_mutation(&mutation)
            .map_err(|e| PersistenceError::with_source(WorldChangeFailure::Failure, e))?
            .len()
            > self.config.transaction_budget
        {
            return Err(PersistenceError::new(WorldChangeFailure::LimitExceeded));
        }
        let mut candidate = state.clone();
        apply(&mut candidate, &mutation);
        restore_validation::checked_checkpoint(&candidate, self.config.byte_budget as usize)?;
        Ok(())
    }
}

impl ExcavationJournalPort for MemoryExcavation {
    fn submit(&self, plan: WorldChangePlan) -> Result<(), PersistenceError> {
        self.validate(&plan)?;
        let mut pending = self
            .pending
            .lock()
            .map_err(|_| PersistenceError::new(WorldChangeFailure::Failure))?;
        if pending.len() >= QUEUE_CAPACITY {
            return Err(PersistenceError::new(WorldChangeFailure::Busy));
        }
        pending.push_back(plan);
        Ok(())
    }
    fn validate(&self, plan: &WorldChangePlan) -> Result<(), PersistenceError> {
        if self.stopped {
            return Err(PersistenceError::new(WorldChangeFailure::Unavailable));
        }
        let state = self
            .lease
            .state
            .lock()
            .map_err(|_| PersistenceError::new(WorldChangeFailure::Failure))?;
        self.check(plan, &state.excavation)
    }
    fn poll_acknowledged(&self) -> Option<Result<WorldChangeAck, PersistenceError>> {
        let mutation = match self.pending.lock() {
            Ok(mut pending) => pending.pop_front()?,
            Err(_) => return Some(Err(PersistenceError::new(WorldChangeFailure::Failure))),
        };
        Some((|| {
            let mut state = self
                .lease
                .state
                .lock()
                .map_err(|_| PersistenceError::new(WorldChangeFailure::Failure))?;
            self.check(&mutation, &state.excavation)?;
            apply(&mut state.excavation, &stored_mutation(&mutation));
            Ok(WorldChangeAck {
                plan: mutation,
                commitment: WorldChangeCommitment::Volatile,
            })
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
        )
        .map_err(io::Error::other)?
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
            if let Some(result) = self.poll_acknowledged() {
                result.map_err(io::Error::other)?;
            }
        }
        self.stopped = true;
        Ok(())
    }
}
