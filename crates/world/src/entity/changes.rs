use super::ids::{ComponentTypeId, EntityId};
use super::registry::ComponentError;
use super::store::ComponentBlob;
use std::collections::BTreeMap;

/// Maximum encoded change history retained for replication clients.
pub const MAX_CHANGE_LOG_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComponentMutation {
    Spawn(EntityId),
    Upsert(EntityId, ComponentTypeId, ComponentBlob),
    RemoveComponent(EntityId, ComponentTypeId),
    Despawn(EntityId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentChange {
    pub revision: u64,
    pub mutation: ComponentMutation,
}

#[derive(Clone, Debug, Default)]
pub(super) struct ChangeLog {
    revision: u64,
    encoded_bytes: usize,
    oldest_revision: u64,
    pub(super) entries: BTreeMap<u64, ComponentChange>,
}

impl ChangeLog {
    pub(super) const fn revision(&self) -> u64 {
        self.revision
    }
    pub(super) const fn oldest_revision(&self) -> u64 {
        self.oldest_revision
    }

    pub(super) fn next_revision(&self) -> Result<u64, ComponentError> {
        self.revision
            .checked_add(1)
            .ok_or(ComponentError::RevisionExhausted)
    }

    pub(super) fn record(&mut self, mutation: ComponentMutation) {
        let revision = self.revision + 1;
        self.revision = revision;
        let bytes = encoded_size(&mutation);
        self.encoded_bytes = self.encoded_bytes.saturating_add(bytes);
        self.entries
            .insert(revision, ComponentChange { revision, mutation });
        while self.encoded_bytes > MAX_CHANGE_LOG_BYTES {
            let Some((&first, change)) = self.entries.first_key_value() else {
                break;
            };
            self.encoded_bytes = self
                .encoded_bytes
                .saturating_sub(encoded_size(&change.mutation));
            self.entries.remove(&first);
            self.oldest_revision = first.saturating_add(1);
        }
    }

    pub(super) fn since(&self, revision: u64) -> impl Iterator<Item = &ComponentChange> {
        self.entries
            .range((revision.saturating_add(1))..)
            .map(|(_, change)| change)
    }
}

fn encoded_size(mutation: &ComponentMutation) -> usize {
    match mutation {
        ComponentMutation::Spawn(_) | ComponentMutation::Despawn(_) => 16,
        ComponentMutation::RemoveComponent(_, _) => 20,
        ComponentMutation::Upsert(_, _, blob) => 24usize.saturating_add(blob.bytes.len()),
    }
}
