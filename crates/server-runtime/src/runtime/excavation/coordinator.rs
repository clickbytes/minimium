//! Sole owner of the bounded prepare -> durable write -> publication transaction.
mod advance;
use super::*;

#[derive(Clone, Copy, Default)]
pub(super) struct Origin {
    pub player: Option<PlayerId>,
    pub context: Option<WorldContext>,
    pub action_id: Option<u64>,
}

pub(super) struct PreparationRequest {
    pub origin: Origin,
    pub world: WorldInstanceId,
    pub impact_bodies: Vec<minimium_world::impact::BodyState>,
    pub input_revision: minimium_procgen::SectorContentVersion,
}

struct Preparation {
    request: PreparationRequest,
    receiver: Receiver<Result<Prepared, PreparationError>>,
    started: Instant,
}

struct Pending {
    origin: Origin,
    prepared: Prepared,
    started: Instant,
}

/// Only this owner can construct the proof consumed by the publication port.
pub(super) struct DurablePublication {
    prepared: Prepared,
    origin: Origin,
}

impl DurablePublication {
    pub(super) fn into_parts(self) -> (Prepared, Origin) {
        (self.prepared, self.origin)
    }
}

pub(super) struct ExcavationCoordinator {
    journal: Box<dyn minimium_storage::excavation::ExcavationJournalPort>,
    preparing: Option<Preparation>,
    pending: Option<Pending>,
    next_id: u64,
    failure: Option<String>,
    shutting_down: bool,
}

impl ExcavationCoordinator {
    pub(super) fn new(
        journal: Box<dyn minimium_storage::excavation::ExcavationJournalPort>,
        last_id: u64,
    ) -> Self {
        Self {
            journal,
            preparing: None,
            pending: None,
            next_id: last_id,
            failure: None,
            shutting_down: false,
        }
    }

    pub(super) fn busy(&self) -> bool {
        self.preparing.is_some() || self.pending.is_some()
    }

    pub(super) fn reserves(&self, world: WorldInstanceId) -> bool {
        self.preparing
            .as_ref()
            .is_some_and(|p| p.request.world == world)
            || self
                .pending
                .as_ref()
                .is_some_and(|p| p.prepared.mutation.world() == world)
    }

    pub(super) fn contains_action(&self, player: PlayerId, context: WorldContext, id: u64) -> bool {
        let matches = |origin: Origin| {
            origin.player == Some(player)
                && origin.context == Some(context)
                && origin.action_id == Some(id)
        };
        self.preparing
            .as_ref()
            .is_some_and(|p| matches(p.request.origin))
            || self.pending.as_ref().is_some_and(|p| matches(p.origin))
    }

    pub(super) fn pending_inventory(&self, profile: PlayerProfileId) -> Option<Inventory> {
        self.pending
            .as_ref()?
            .prepared
            .mutation
            .inventory()
            .filter(|(owner, _)| *owner == profile)
            .map(|(_, inventory)| inventory)
    }

    pub(super) fn allocate_id(&mut self) -> Option<u64> {
        self.next_id = self.next_id.checked_add(1)?;
        Some(self.next_id)
    }

    pub(super) fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    pub(super) fn fail(&mut self, reason: String) {
        self.failure = Some(reason);
    }

    pub(super) fn storage_rejection(
        &mut self,
        error: minimium_storage::excavation::PersistenceError,
    ) -> ActionRejection {
        use minimium_gameplay::WorldChangeFailure as Failure;
        match error.kind {
            Failure::IdentityConflict | Failure::RevisionConflict => ActionRejection::Stale,
            Failure::LimitExceeded | Failure::InvalidPlan => ActionRejection::Capacity,
            Failure::Busy => ActionRejection::Busy,
            Failure::Unavailable | Failure::Failure => {
                let detail = error
                    .source_error()
                    .map_or_else(|| format!("{:?}", error.kind), ToString::to_string);
                self.fail(detail);
                ActionRejection::Storage
            }
        }
    }

    pub(super) fn prepare(
        &mut self,
        request: PreparationRequest,
        job: Box<dyn FnOnce() -> Result<Prepared, PreparationError> + Send>,
        work: &mut super::ports::WorkSubmission<'_>,
    ) -> io::Result<()> {
        if self.busy() || self.shutting_down {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "excavation transaction is reserved",
            ));
        }
        if let Some(error) = self.failure() {
            return Err(io::Error::other(error.to_owned()));
        }
        let receiver = work.submit(&request.impact_bodies, job)?;
        self.preparing = Some(Preparation {
            request,
            receiver,
            started: Instant::now(),
        });
        Ok(())
    }

    /// Queue acceptance reserves the slot; only advance can authorize publication.
    pub(super) fn submit(
        &mut self,
        prepared: Prepared,
        origin: Origin,
    ) -> Result<(), minimium_storage::excavation::PersistenceError> {
        if self.busy() {
            return Err(minimium_storage::excavation::PersistenceError::new(
                minimium_gameplay::WorldChangeFailure::Busy,
            ));
        }
        self.journal.submit(prepared.mutation.clone())?;
        self.pending = Some(Pending {
            origin,
            prepared,
            started: Instant::now(),
        });
        Ok(())
    }

    pub(super) fn shutting_down(&self) -> bool {
        self.shutting_down
    }

    pub(super) fn begin_shutdown(&mut self, work: &mut super::ports::WorkSubmission<'_>) {
        // The shared worker pool owns joining outstanding tasks. Their dropped
        // result receiver cannot publish an unsubmitted mutation.
        if let Some(preparation) = self.preparing.take() {
            work.cancel(&preparation.request.impact_bodies);
        }
        self.shutting_down = true;
    }

    pub(super) fn awaiting_durability(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn shutdown_journal(&mut self, deadline: Instant) -> io::Result<()> {
        self.journal.shutdown_until(deadline)
    }
}
