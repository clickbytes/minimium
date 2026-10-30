//! Bounded, I/O-free sector request, ticket, readiness and cache lifecycle.

use minimium_core::world::{
    GenerationRevision, SectorKey, SectorRevision, WorldContentRevision, WorldInstanceId,
};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct SectorAddress {
    instance: WorldInstanceId,
    key: SectorKey,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SectorTicket {
    address: SectorAddress,
    generation: GenerationRevision,
    content: WorldContentRevision,
    serial: u64,
}

impl SectorTicket {
    pub const fn instance(self) -> WorldInstanceId {
        self.address.instance
    }

    pub const fn key(self) -> SectorKey {
        self.address.key
    }

    pub const fn generation(self) -> GenerationRevision {
        self.generation
    }

    pub const fn content_revision(self) -> WorldContentRevision {
        self.content
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SectorState {
    Queued,
    Generating,
    DataReady,
    Installed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestError {
    InvalidCapacity,
    QueueFull,
    CacheFull,
    TicketSpaceExhausted,
    SectorInUse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallResult {
    Installed,
    Stale,
    WrongState,
}

struct SectorEntry<T> {
    ticket: SectorTicket,
    state: SectorState,
    payload: Option<Arc<T>>,
    revision: Option<SectorRevision>,
    collision_ready: bool,
    dirty: bool,
    pins: u32,
}

/// Own this manager on one mutation lane. Worker jobs receive copyable tickets
/// and immutable generation inputs; only the owner installs returned payloads.
pub struct SectorManager<T> {
    entries: BTreeMap<SectorAddress, SectorEntry<T>>,
    pending: VecDeque<SectorTicket>,
    cache_capacity: usize,
    queue_capacity: usize,
    next_serial: u64,
}

impl<T> SectorManager<T> {
    pub fn new(cache_capacity: usize, queue_capacity: usize) -> Result<Self, RequestError> {
        if cache_capacity == 0 || queue_capacity == 0 {
            return Err(RequestError::InvalidCapacity);
        }
        Ok(Self {
            entries: BTreeMap::new(),
            pending: VecDeque::with_capacity(queue_capacity),
            cache_capacity,
            queue_capacity,
            next_serial: 1,
        })
    }

    pub fn request(
        &mut self,
        instance: WorldInstanceId,
        key: SectorKey,
        generation: GenerationRevision,
        content: WorldContentRevision,
    ) -> Result<SectorTicket, RequestError> {
        let address = SectorAddress { instance, key };
        let existing = self.entries.get(&address);
        if let Some(existing) = existing {
            if existing.ticket.generation == generation && existing.ticket.content == content {
                return Ok(existing.ticket);
            }
            if existing.pins > 0 || existing.dirty {
                return Err(RequestError::SectorInUse);
            }
        }
        let old_queued = existing.is_some_and(|entry| entry.state == SectorState::Queued);
        let pending_after_replace = self.pending.len() - usize::from(old_queued);
        if pending_after_replace >= self.queue_capacity {
            return Err(RequestError::QueueFull);
        }
        if existing.is_none() && self.entries.len() >= self.cache_capacity {
            return Err(RequestError::CacheFull);
        }
        let serial = self.next_serial;
        self.next_serial = serial
            .checked_add(1)
            .ok_or(RequestError::TicketSpaceExhausted)?;
        if let Some(existing) = existing {
            // Replacing a generating entry invalidates its ticket. A worker may
            // still return later, but install_data will reject that stale result.
            self.pending.retain(|ticket| *ticket != existing.ticket);
            self.entries.remove(&address);
        }
        let ticket = SectorTicket {
            address,
            generation,
            content,
            serial,
        };
        self.entries.insert(
            address,
            SectorEntry {
                ticket,
                state: SectorState::Queued,
                payload: None,
                revision: None,
                collision_ready: false,
                dirty: false,
                pins: 0,
            },
        );
        self.pending.push_back(ticket);
        Ok(ticket)
    }

    pub fn begin_next(&mut self) -> Option<SectorTicket> {
        while let Some(ticket) = self.pending.pop_front() {
            let Some(entry) = self.entries.get_mut(&ticket.address) else {
                continue;
            };
            if entry.ticket == ticket && entry.state == SectorState::Queued {
                entry.state = SectorState::Generating;
                return Some(ticket);
            }
        }
        None
    }

    pub fn install_data(
        &mut self,
        ticket: SectorTicket,
        revision: SectorRevision,
        payload: Arc<T>,
    ) -> InstallResult {
        let Some(entry) = self.entries.get_mut(&ticket.address) else {
            return InstallResult::Stale;
        };
        if entry.ticket != ticket {
            return InstallResult::Stale;
        }
        if entry.state != SectorState::Generating {
            return InstallResult::WrongState;
        }
        entry.payload = Some(payload);
        entry.revision = Some(revision);
        entry.state = SectorState::DataReady;
        InstallResult::Installed
    }

    pub fn install_collision(&mut self, ticket: SectorTicket) -> InstallResult {
        let Some(entry) = self.entries.get_mut(&ticket.address) else {
            return InstallResult::Stale;
        };
        if entry.ticket != ticket {
            return InstallResult::Stale;
        }
        if entry.state != SectorState::DataReady {
            return InstallResult::WrongState;
        }
        entry.collision_ready = true;
        entry.state = SectorState::Installed;
        InstallResult::Installed
    }

    pub fn state(&self, ticket: SectorTicket) -> Option<SectorState> {
        let entry = self.entries.get(&ticket.address)?;
        (entry.ticket == ticket).then_some(entry.state)
    }

    pub fn payload(&self, ticket: SectorTicket) -> Option<&T> {
        let entry = self.entries.get(&ticket.address)?;
        if entry.ticket == ticket {
            entry.payload.as_deref()
        } else {
            None
        }
    }

    pub fn payload_arc(&self, ticket: SectorTicket) -> Option<Arc<T>> {
        let entry = self.entries.get(&ticket.address)?;
        (entry.ticket == ticket)
            .then(|| entry.payload.as_ref().map(Arc::clone))
            .flatten()
    }

    /// Iterates immutable payloads that have completed both data and collision
    /// installation. Mutation and eviction remain on the manager's owner lane.
    pub fn installed(&self) -> impl Iterator<Item = (SectorTicket, SectorRevision, &T)> {
        self.entries.values().filter_map(|entry| {
            if entry.state != SectorState::Installed || !entry.collision_ready {
                return None;
            }
            Some((entry.ticket, entry.revision?, entry.payload.as_deref()?))
        })
    }

    pub fn collision_ready(&self, ticket: SectorTicket) -> bool {
        self.entries
            .get(&ticket.address)
            .is_some_and(|entry| entry.ticket == ticket && entry.collision_ready)
    }

    pub fn mark_dirty(&mut self, ticket: SectorTicket) -> bool {
        let Some(entry) = self.entries.get_mut(&ticket.address) else {
            return false;
        };
        if entry.ticket != ticket || entry.payload.is_none() {
            return false;
        }
        entry.dirty = true;
        true
    }

    pub fn mark_saved(&mut self, ticket: SectorTicket, revision: SectorRevision) -> bool {
        let Some(entry) = self.entries.get_mut(&ticket.address) else {
            return false;
        };
        if entry.ticket != ticket || entry.revision != Some(revision) {
            return false;
        }
        entry.dirty = false;
        true
    }

    pub fn pin(&mut self, ticket: SectorTicket) -> bool {
        let Some(entry) = self.entries.get_mut(&ticket.address) else {
            return false;
        };
        if entry.ticket != ticket || entry.pins == u32::MAX {
            return false;
        }
        entry.pins += 1;
        true
    }

    pub fn unpin(&mut self, ticket: SectorTicket) -> bool {
        let Some(entry) = self.entries.get_mut(&ticket.address) else {
            return false;
        };
        if entry.ticket != ticket || entry.pins == 0 {
            return false;
        }
        entry.pins -= 1;
        true
    }

    pub fn evict(&mut self, ticket: SectorTicket) -> bool {
        let Some(entry) = self.entries.get(&ticket.address) else {
            return false;
        };
        if entry.ticket != ticket
            || entry.pins != 0
            || entry.dirty
            || entry.state == SectorState::Generating
        {
            return false;
        }
        self.pending.retain(|pending| *pending != ticket);
        self.entries.remove(&ticket.address).is_some()
    }

    /// Evicts the oldest clean, unpinned entry that is not being generated.
    pub fn evict_oldest(&mut self) -> bool {
        let candidate = self
            .entries
            .iter()
            .filter(|(_, entry)| {
                entry.pins == 0 && !entry.dirty && entry.state != SectorState::Generating
            })
            .min_by_key(|(_, entry)| entry.ticket.serial)
            .map(|(address, entry)| (*address, entry.ticket));
        let Some((address, ticket)) = candidate else {
            return false;
        };
        self.pending.retain(|pending| *pending != ticket);
        self.entries.remove(&address).is_some()
    }

    /// Releases every clean, unpinned sector in an instance and leaves busy
    /// entries available for their current owners to finish or save.
    pub fn release_instance(&mut self, instance: WorldInstanceId) -> usize {
        let releasable: Vec<_> = self
            .entries
            .iter()
            .filter_map(|(address, entry)| {
                (address.instance == instance
                    && entry.pins == 0
                    && !entry.dirty
                    && entry.state != SectorState::Generating)
                    .then_some((*address, entry.ticket))
            })
            .collect();
        for (address, ticket) in &releasable {
            self.pending.retain(|pending| pending != ticket);
            self.entries.remove(address);
        }
        releasable.len()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn cached_count(&self) -> usize {
        self.entries.len()
    }
}
