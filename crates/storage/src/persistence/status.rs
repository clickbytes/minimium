use crate::compact_error;
use std::io;

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
}

impl Default for SaveBookkeeping {
    fn default() -> Self {
        Self {
            pending_saves: 0,
            last_save: SaveStatus::Never,
        }
    }
}

impl SaveBookkeeping {
    pub(super) fn record_queued(&mut self) {
        self.pending_saves = self.pending_saves.saturating_add(1);
        self.last_save = SaveStatus::Pending;
    }

    pub(super) fn record_enqueue_failure(&mut self, error: &io::Error) {
        self.last_save = SaveStatus::Failed(compact_error(error));
    }

    pub(super) fn record_completion(&mut self, result: io::Result<()>) {
        self.pending_saves = self.pending_saves.saturating_sub(1);
        match result {
            Ok(()) => {
                self.last_save = SaveStatus::Complete;
                println!("world save completed");
            }
            Err(error) => {
                self.last_save = SaveStatus::Failed(compact_error(&error));
                eprintln!("world save failed: {error}");
            }
        }
    }

    pub(super) fn record_worker_stopped(&mut self) {
        if self.pending_saves > 0 {
            self.pending_saves = 0;
            self.last_save = SaveStatus::Failed(String::from("world_storage_worker_stopped"));
            eprintln!("world storage worker stopped before completing a save");
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
}
