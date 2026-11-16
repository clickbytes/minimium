//! Bounded, host-owned volatile persistence. Acknowledgements mean committed to
//! this backend's memory; they do not promise survival of process termination.
use crate::{excavation::*, impact_journal::MemoryImpacts, *};
use minimium_gameplay::WorldChangeState;
use minimium_world::{entity::ComponentRegistry, impact::BodyState, world::*};
use std::{
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};
mod snapshot;

#[derive(Clone, Copy, Debug)]
pub struct MemoryStorageLimits {
    pub snapshot_bytes: usize,
    pub excavation: JournalConfig,
    pub impact_bodies: usize,
}
impl Default for MemoryStorageLimits {
    fn default() -> Self {
        Self {
            snapshot_bytes: 128 * 1024 * 1024,
            excavation: JournalConfig::default(),
            impact_bodies: 4096,
        }
    }
}
#[derive(Default)]
pub(crate) struct State {
    snapshot: Option<Vec<u8>>,
    pub(crate) excavation: WorldChangeState,
    pub(crate) impacts: BTreeMap<(WorldInstanceId, u64), BodyState>,
    opened: bool,
}
pub(crate) struct Lease {
    pub(crate) state: Arc<Mutex<State>>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .opened = false;
    }
}

/// Clones share one committed image and one exclusive opened-service lease.
/// Keep a clone to reopen after all ports have been dropped. Loading while a
/// runtime owns the ports is rejected, so snapshots and journals cannot be mixed.
#[derive(Clone)]
pub struct MemoryStorageBackend {
    state: Arc<Mutex<State>>,
    limits: MemoryStorageLimits,
}
impl MemoryStorageBackend {
    pub fn new(limits: MemoryStorageLimits) -> io::Result<Self> {
        if !(1024..=128 * 1024 * 1024).contains(&limits.snapshot_bytes)
            || !(1024 * 1024..=MAX_JOURNAL_BYTES).contains(&limits.excavation.byte_budget)
            || !(1024..=MAX_MUTATION_BYTES).contains(&limits.excavation.transaction_budget)
            || !(1..=4096).contains(&limits.impact_bodies)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid memory storage limits",
            ));
        }
        Ok(Self {
            state: Arc::new(Mutex::new(State::default())),
            limits,
        })
    }
}
impl StorageBackend for MemoryStorageBackend {
    fn preflight(&self) -> io::Result<()> {
        let state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("memory backend poisoned"))?;
        if state.opened {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "memory backend is in use",
            ));
        }
        Ok(())
    }
    fn load(
        &self,
        worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
        components: &ComponentRegistry,
    ) -> Result<LoadedState, StorageError> {
        let state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("memory backend poisoned"))?;
        if state.opened {
            return Err(StorageError::busy("memory backend is in use"));
        }
        match &state.snapshot {
            Some(bytes) => crate::format::load_bytes(bytes.clone(), worlds, components)
                .map_err(StorageError::from),
            None => Ok(LoadedState::empty()),
        }
    }
    fn open_services(&self) -> io::Result<StorageServices> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("memory backend poisoned"))?;
        if state.opened {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "memory backend is in use",
            ));
        }
        let excavation_restore = state.excavation.clone();
        let impact_restore = state.impacts.values().copied().collect();
        state.opened = true;
        let lease = Arc::new(Lease {
            state: self.state.clone(),
        });
        Ok(StorageServices {
            persistence: Box::new(snapshot::MemorySnapshots::new(
                lease.clone(),
                self.limits.snapshot_bytes,
            )),
            excavation: Box::new(MemoryExcavation::new(lease.clone(), self.limits.excavation)),
            excavation_restore,
            impacts: Box::new(MemoryImpacts::new(lease, self.limits.impact_bodies)),
            impact_restore,
        })
    }
}
