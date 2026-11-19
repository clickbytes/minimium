//! Runtime-scoped identity and bounded history for accepted snapshot saves.
use crate::storage::{SaveCompletion, SaveDurability, SaveJobId};
use std::{
    collections::VecDeque,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveSource {
    Manual,
    Autosave,
    Shutdown,
}

#[derive(Clone, Debug)]
pub enum SaveJobState {
    Queued,
    Completed(SaveDurability),
    Failed { code: String, detail: String },
}

#[derive(Clone, Debug)]
pub struct SaveJobRecord {
    pub id: SaveJobId,
    pub source: SaveSource,
    pub snapshot_revision: u64,
    pub committed_tick: u64,
    pub journal_revision: u64,
    pub world_count: usize,
    pub state: SaveJobState,
    pub queued_at_ms: u64,
    pub finished_at_ms: Option<u64>,
}

#[derive(Clone, Debug)]
pub enum SaveJobLookup {
    Never,
    Tracked(SaveJobRecord),
    Untracked,
    Expired,
}

#[derive(Clone, Debug)]
pub struct SaveJobsStatus {
    pub active: Option<SaveJobRecord>,
    pub last: Option<SaveJobRecord>,
    pub lookup: SaveJobLookup,
}

pub(crate) struct PersistenceJobs {
    nonce: u64,
    next: u64,
    started: Instant,
    active: Option<SaveJobRecord>,
    history: VecDeque<(Instant, SaveJobRecord)>,
}

impl PersistenceJobs {
    pub(crate) fn new(started: Instant) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |time| time.as_nanos() as u64)
            ^ u64::from(std::process::id());
        Self {
            nonce,
            next: 1,
            started,
            active: None,
            history: VecDeque::new(),
        }
    }

    pub(crate) fn allocate(&mut self) -> SaveJobId {
        let id = SaveJobId((u128::from(self.nonce) << 64) | u128::from(self.next));
        self.next = self
            .next
            .checked_add(1)
            .expect("save job ID space exhausted");
        id
    }

    pub(crate) fn has_active(&self) -> bool {
        self.active.is_some()
    }

    pub(crate) fn queued(
        &mut self,
        id: SaveJobId,
        source: SaveSource,
        snapshot: &crate::storage::SaveSnapshot,
        now: Instant,
    ) -> SaveJobRecord {
        self.prune(now);
        let record = SaveJobRecord {
            id,
            source,
            snapshot_revision: snapshot.revision(),
            committed_tick: snapshot.committed_tick(),
            journal_revision: snapshot.journal_revision(),
            world_count: snapshot.world_count(),
            state: SaveJobState::Queued,
            queued_at_ms: self.ms(now),
            finished_at_ms: None,
        };
        self.active = Some(record.clone());
        record
    }

    pub(crate) fn complete(&mut self, completion: SaveCompletion, now: Instant) {
        let Some(mut record) = self.active.take() else {
            return;
        };
        if record.id != completion.id || record.snapshot_revision != completion.revision {
            self.active = Some(record);
            return;
        }
        record.state = match completion.result {
            Ok(()) => SaveJobState::Completed(completion.durability),
            Err(error) => SaveJobState::Failed {
                code: error.code().to_owned(),
                detail: bounded_detail(&error),
            },
        };
        record.finished_at_ms = Some(self.ms(now));
        self.history.push_front((now, record));
        self.prune(now);
    }

    pub(crate) fn fail(
        &mut self,
        id: SaveJobId,
        error: &crate::storage::StorageError,
        now: Instant,
    ) {
        let Some(mut record) = self.active.take() else {
            return;
        };
        if record.id != id {
            self.active = Some(record);
            return;
        }
        record.state = SaveJobState::Failed {
            code: error.code().to_owned(),
            detail: bounded_detail(error),
        };
        record.finished_at_ms = Some(self.ms(now));
        self.history.push_front((now, record));
        self.prune(now);
    }

    pub(super) fn status(&self, id: Option<SaveJobId>, now: Instant) -> SaveJobsStatus {
        let last = self
            .history
            .iter()
            .find(|(at, _)| now.saturating_duration_since(*at) <= Duration::from_secs(600))
            .map(|(_, record)| record.clone());
        let lookup = match id {
            None => self
                .active
                .clone()
                .or_else(|| last.clone())
                .map_or(SaveJobLookup::Never, SaveJobLookup::Tracked),
            Some(id) => self
                .active
                .as_ref()
                .filter(|r| r.id == id)
                .or_else(|| {
                    self.history
                        .iter()
                        .find(|(at, r)| {
                            r.id == id
                                && now.saturating_duration_since(*at) <= Duration::from_secs(600)
                        })
                        .map(|(_, r)| r)
                })
                .cloned()
                .map_or_else(
                    || {
                        if (id.0 >> 64) as u64 == self.nonce && (id.0 as u64) < self.next {
                            SaveJobLookup::Expired
                        } else {
                            SaveJobLookup::Untracked
                        }
                    },
                    SaveJobLookup::Tracked,
                ),
        };
        SaveJobsStatus {
            active: self.active.clone(),
            last,
            lookup,
        }
    }

    fn ms(&self, now: Instant) -> u64 {
        u64::try_from(now.saturating_duration_since(self.started).as_millis()).unwrap_or(u64::MAX)
    }

    fn prune(&mut self, now: Instant) {
        while self.history.len() > 64
            || self.history.back().is_some_and(|(at, _)| {
                now.saturating_duration_since(*at) > Duration::from_secs(600)
            })
        {
            self.history.pop_back();
        }
    }
}

fn bounded_detail(error: &crate::storage::StorageError) -> String {
    let mut detail = error.to_string();
    if detail.len() > 1024 {
        let mut end = 1024;
        while !detail.is_char_boundary(end) {
            end -= 1;
        }
        detail.truncate(end);
    }
    detail
}

impl super::ServerRuntime {
    pub(super) fn queue_manual_save(
        &mut self,
        monotonic_ms: minimium_environment::time::MonotonicMs,
    ) -> crate::commands::CommandReply {
        use crate::commands::{CommandPayload, CommandReply};
        if self.operations.save_jobs.has_active() {
            return CommandReply::error("storage.busy", "a world save is already pending");
        }
        if self.operations.persistence.has_pending_save() {
            let error = self.operations.persistence.reject_pending_save();
            return CommandReply::error(error.code(), &error.to_string());
        }
        let saved_clock = match self.environment_clock.capture_saved_state(monotonic_ms) {
            Ok(clock) => clock,
            Err(error) => return CommandReply::error(error.code(), &error.to_string()),
        };
        let input = match self.game.worlds.snapshot_for_save(self.map.fingerprint()) {
            Ok(input) => input,
            Err(error) => {
                return CommandReply::error("server.world_snapshot_failed", &error.to_string())
            }
        };
        let snapshot = match self
            .operations
            .player_store
            .save_snapshot(input, saved_clock)
        {
            Ok(snapshot) => snapshot,
            Err(error) => return CommandReply::error(error.code(), &error.to_string()),
        };
        let id = self.operations.save_jobs.allocate();
        let record = self
            .operations
            .save_jobs
            .queued(id, SaveSource::Manual, &snapshot, self.now);
        match self.operations.persistence.queue_save(id, snapshot) {
            Ok(()) => CommandReply::success(CommandPayload::SaveJob(record)),
            Err(error) => {
                self.operations.save_jobs.reject_queued(id);
                CommandReply::error(error.code(), &error.to_string())
            }
        }
    }
}

impl PersistenceJobs {
    pub(super) fn reject_queued(&mut self, id: SaveJobId) {
        if self.active.as_ref().is_some_and(|record| record.id == id) {
            self.active = None;
        }
    }
}
