use minimium_core::identity::PlayerProfileId;
use minimium_world::entity::{ComponentMutation, ComponentTypeId, EntityId, WorldEntityChange};
use minimium_world::world::{SectorKey, WorldInstanceId};
use std::{collections::HashMap, io};

const PROFILE_BUDGET_BYTES: usize = 64 * 1024 * 1024;
const TOTAL_BUDGET_BYTES: usize = 128 * 1024 * 1024;
pub const MAX_SNAPSHOT_WORLDS: usize = 16_384;
pub(crate) const MAX_WORLD_METADATA_BYTES: usize = 1024;
pub(crate) const FIXED_SNAPSHOT_HEADER_RESERVE_BYTES: usize = 64 * 1024;
const FIXED_RECORD_RESERVE_BYTES: usize =
    MAX_SNAPSHOT_WORLDS * MAX_WORLD_METADATA_BYTES + FIXED_SNAPSHOT_HEADER_RESERVE_BYTES;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SnapshotRecordKey {
    Profile(PlayerProfileId),
    Entity(WorldInstanceId, EntityId),
    Component(WorldInstanceId, EntityId, ComponentTypeId),
    Sector(WorldInstanceId, SectorKey),
}

#[derive(Clone, Debug)]
pub struct SnapshotBudget {
    limit: usize,
    reserved: usize,
    reserved_profiles: usize,
    records: HashMap<SnapshotRecordKey, usize>,
    component_payloads: HashMap<SnapshotRecordKey, usize>,
    reserved_component_payloads: usize,
}

/// A fully checked replacement budget that holds an exclusive borrow until commit or drop.
#[must_use = "a prepared entity reservation only changes the budget when committed"]
pub struct PreparedEntityReservation<'a> {
    target: &'a mut SnapshotBudget,
    staged: Option<SnapshotBudget>,
}

impl PreparedEntityReservation<'_> {
    /// Commits the checked budget candidate. The exclusive borrow prevents stale application.
    pub fn commit(self) {
        if let Some(staged) = self.staged {
            *self.target = staged;
        }
    }
}

impl Default for SnapshotBudget {
    fn default() -> Self {
        Self::new(TOTAL_BUDGET_BYTES)
    }
}

impl SnapshotBudget {
    pub fn new(limit: usize) -> Self {
        let limit = limit.min(TOTAL_BUDGET_BYTES);
        Self {
            limit,
            reserved: limit.min(FIXED_RECORD_RESERVE_BYTES),
            reserved_profiles: 0,
            records: HashMap::new(),
            component_payloads: HashMap::new(),
            reserved_component_payloads: 0,
        }
    }

    pub fn reserve(&mut self, key: SnapshotRecordKey, bytes: usize) -> io::Result<()> {
        let previous = self.records.get(&key).copied().unwrap_or(0);
        let next = self
            .reserved
            .checked_sub(previous)
            .and_then(|value| value.checked_add(bytes))
            .ok_or_else(capacity_error)?;
        if next > self.limit {
            return Err(capacity_error());
        }
        let profiles = if matches!(key, SnapshotRecordKey::Profile(_)) {
            self.reserved_profiles
                .checked_sub(previous)
                .and_then(|value| value.checked_add(bytes))
                .ok_or_else(capacity_error)?
        } else {
            self.reserved_profiles
        };
        if profiles > PROFILE_BUDGET_BYTES {
            return Err(capacity_error());
        }
        self.records.insert(key, bytes);
        self.reserved = next;
        self.reserved_profiles = profiles;
        Ok(())
    }

    pub fn reserve_many(
        &mut self,
        reservations: impl IntoIterator<Item = (SnapshotRecordKey, usize)>,
    ) -> io::Result<()> {
        let mut candidate = self.clone();
        for (key, bytes) in reservations {
            candidate.reserve(key, bytes)?;
        }
        *self = candidate;
        Ok(())
    }

    pub fn reserve_component_payloads(
        &mut self,
        reservations: impl IntoIterator<Item = (SnapshotRecordKey, usize)>,
    ) -> io::Result<()> {
        let mut candidate = self.clone();
        for (key, bytes) in reservations {
            if !matches!(key, SnapshotRecordKey::Component(_, _, _))
                || !candidate.records.contains_key(&key)
                || bytes > minimium_world::entity::MAX_COMPONENT_BYTES
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "component payload has no record reservation",
                ));
            }
            let previous = candidate.component_payloads.get(&key).copied().unwrap_or(0);
            let total = candidate
                .reserved_component_payloads
                .checked_sub(previous)
                .and_then(|total| total.checked_add(bytes))
                .ok_or_else(capacity_error)?;
            if total > minimium_world::entity::MAX_COMPONENT_PAYLOAD_BYTES {
                return Err(capacity_error());
            }
            candidate.component_payloads.insert(key, bytes);
            candidate.reserved_component_payloads = total;
        }
        *self = candidate;
        Ok(())
    }

    /// Measures and reserves an ordered entity-store change batch without mutating this budget.
    /// The returned guard keeps this budget exclusively borrowed until the caller commits it or
    /// drops it after another part of the transaction fails.
    pub fn prepare_entity_changes<'a>(
        &'a mut self,
        changes: &[WorldEntityChange],
    ) -> io::Result<PreparedEntityReservation<'a>> {
        if changes.is_empty() {
            return Ok(PreparedEntityReservation {
                target: self,
                staged: None,
            });
        }
        let mut staged = self.clone();
        for change in changes {
            match &change.mutation {
                ComponentMutation::Spawn(entity) => staged.reserve(
                    SnapshotRecordKey::Entity(change.world, *entity),
                    crate::entity_record_bytes(change.world, *entity)?,
                )?,
                ComponentMutation::Upsert(entity, kind, blob) => {
                    let key = SnapshotRecordKey::Component(change.world, *entity, *kind);
                    staged.reserve(
                        key,
                        crate::component_record_bytes(change.world, *entity, *kind, blob)?,
                    )?;
                    staged.reserve_component_payloads([(key, blob.bytes.len())])?;
                }
                ComponentMutation::RemoveComponent(entity, kind) => {
                    staged.release(SnapshotRecordKey::Component(change.world, *entity, *kind));
                }
                ComponentMutation::Despawn(entity) => {
                    staged.release_entity(change.world, *entity);
                }
            }
        }
        Ok(PreparedEntityReservation {
            target: self,
            staged: Some(staged),
        })
    }

    pub fn release(&mut self, key: SnapshotRecordKey) {
        if let Some(bytes) = self.records.remove(&key) {
            self.reserved = self.reserved.saturating_sub(bytes);
            if matches!(key, SnapshotRecordKey::Profile(_)) {
                self.reserved_profiles = self.reserved_profiles.saturating_sub(bytes);
            }
        }
        if let Some(bytes) = self.component_payloads.remove(&key) {
            self.reserved_component_payloads =
                self.reserved_component_payloads.saturating_sub(bytes);
        }
    }

    fn release_entity(&mut self, world: WorldInstanceId, entity: EntityId) {
        self.release(SnapshotRecordKey::Entity(world, entity));
        let components = self
            .records
            .keys()
            .filter_map(|key| match key {
                SnapshotRecordKey::Component(record_world, record_entity, _)
                    if *record_world == world && *record_entity == entity =>
                {
                    Some(*key)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for component in components {
            self.release(component);
        }
    }

    pub const fn reserved_bytes(&self) -> usize {
        self.reserved
    }
}

fn capacity_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::FileTooLarge,
        "snapshot budget reservation exceeded",
    )
}
