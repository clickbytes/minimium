use crate::compact_error;
use crate::StorageError;
use minimium_diagnostics::{DomainLogger, EventCode, Level};

enum SaveStatus {
    Never,
    Pending,
    Complete,
    Failed(String),
}

impl std::fmt::Display for SaveStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Never => f.write_str("never"),
            Self::Pending => f.write_str("pending"),
            Self::Complete => f.write_str("ok"),
            Self::Failed(message) => write!(f, "error_{message}"),
        }
    }
}

pub(super) struct SaveBookkeeping {
    pending_saves: usize,
    last_save: SaveStatus,
    logger: DomainLogger,
}

impl SaveBookkeeping {
    pub(super) fn new(logger: DomainLogger) -> Self {
        Self {
            pending_saves: 0,
            last_save: SaveStatus::Never,
            logger,
        }
    }
    pub(super) fn record_queued(&mut self) {
        self.pending_saves = self.pending_saves.saturating_add(1);
        self.last_save = SaveStatus::Pending;
        self.emit("storage.save_queued", Level::Info, "pending".to_owned());
    }

    pub(super) fn record_enqueue_failure(&mut self, error: &StorageError) {
        self.last_save = SaveStatus::Failed(compact_error(error));
        self.emit(
            "storage.save_enqueue_failed",
            Level::Error,
            compact_error(error),
        );
    }

    pub(super) fn record_completion(&mut self, result: Result<(), StorageError>) {
        self.pending_saves = self.pending_saves.saturating_sub(1);
        match result {
            Ok(()) => {
                self.last_save = SaveStatus::Complete;
                self.emit("storage.save_completed", Level::Info, "ok".to_owned());
            }
            Err(error) => {
                self.last_save = SaveStatus::Failed(compact_error(&error));
                self.emit("storage.save_failed", Level::Error, compact_error(&error));
            }
        }
    }

    pub(super) fn record_worker_stopped(&mut self) {
        if self.pending_saves > 0 {
            self.pending_saves = 0;
            self.last_save = SaveStatus::Failed(String::from("world_storage_worker_stopped"));
            self.emit(
                "storage.worker_stopped",
                Level::Error,
                "pending_save_lost".to_owned(),
            );
        }
    }

    pub(super) fn has_pending_save(&self) -> bool {
        self.pending_saves != 0
    }

    pub(super) fn summary(&self) -> String {
        format!(
            "save_pending={} save_last={}",
            self.pending_saves, self.last_save
        )
    }

    pub(super) fn emit(&self, code: &'static str, level: Level, result: String) {
        let Ok(code) = EventCode::new(code) else {
            return;
        };
        self.logger
            .emit(self.logger.event(level, code).field("result", &result));
    }
}
