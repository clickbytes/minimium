//! Host-selected persistence bootstrap and durable service ports.
use crate::{
    excavation::{ExcavationJournal, ExcavationJournalPort, JOURNAL_FILE_NAME},
    impact_journal::{ImpactJournal, ImpactJournalPort},
    LoadedState, PersistencePort, PersistenceWorker, PlayerStore, StorageError,
};
use minimium_diagnostics::DomainLogger;
use minimium_gameplay::WorldChangeState;
use minimium_world::{
    entity::ComponentRegistry,
    impact::BodyState,
    world::{WorldInstanceId, WorldManifest, WorldMap},
};
use std::{
    io,
    path::{Path, PathBuf},
};

/// Opened persistence services passed to a runtime after host path/lock
/// bootstrap; durable worker I/O remains owned by these services.
pub struct StorageServices {
    pub persistence: Box<dyn PersistencePort>,
    pub excavation: Box<dyn ExcavationJournalPort>,
    pub excavation_restore: WorldChangeState,
    pub impacts: Box<dyn ImpactJournalPort>,
    pub impact_restore: Vec<BodyState>,
}

/// Host-selected storage backend. Alternate backends need no filesystem path;
/// the file implementation stores its root in the adapter itself.
pub trait StorageBackend: Send + Sync {
    fn preflight(&self) -> io::Result<()>;
    fn load(
        &self,
        worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
        components: &ComponentRegistry,
    ) -> Result<LoadedState, StorageError>;
    fn open_services(&self) -> io::Result<StorageServices>;
}

/// Filesystem-backed adapter used by the standalone server process.
pub struct FileStorageBackend {
    root: PathBuf,
    logger: DomainLogger,
}

impl FileStorageBackend {
    pub fn new(root: impl Into<PathBuf>, logger: DomainLogger) -> Self {
        Self {
            root: root.into(),
            logger,
        }
    }

    pub fn preflight_path(root: &Path) -> io::Result<()> {
        crate::preflight_snapshot_version(root)?;
        ExcavationJournal::preflight(&root.join(JOURNAL_FILE_NAME))
    }
}

impl StorageBackend for FileStorageBackend {
    fn preflight(&self) -> io::Result<()> {
        Self::preflight_path(&self.root)
    }

    fn load(
        &self,
        worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
        components: &ComponentRegistry,
    ) -> Result<LoadedState, StorageError> {
        PlayerStore::load(&self.root, worlds, components)
    }

    fn open_services(&self) -> io::Result<StorageServices> {
        let persistence = PersistenceWorker::spawn(self.root.clone(), self.logger.clone())?;
        let (excavation, excavation_restore) =
            ExcavationJournal::open(&self.root.join(JOURNAL_FILE_NAME))?;
        let (impacts, impact_restore) = ImpactJournal::open(&self.root)?;
        Ok(StorageServices {
            persistence: Box::new(persistence),
            excavation: Box::new(excavation),
            excavation_restore,
            impacts: Box::new(impacts),
            impact_restore,
        })
    }
}
