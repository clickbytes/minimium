use super::{CatalogEntry, OperationId};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_OPERATIONS: usize = 1024;
pub const MAX_ALIASES: usize = 2048;
const MAX_NAME: usize = 128;
const MAX_DEPTH: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CatalogError {
    InvalidName(String),
    Capacity,
    Collision {
        name: String,
        existing: (OperationId, &'static str),
        incoming: (OperationId, &'static str),
    },
}

impl std::fmt::Display for CatalogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "operation catalog registration failed: {self:?}")
    }
}
impl std::error::Error for CatalogError {}

#[derive(Clone, Default)]
pub struct OperationCatalog {
    entries: BTreeMap<OperationId, CatalogEntry>,
    names: BTreeMap<String, OperationId>,
    namespaces: BTreeMap<String, OperationId>,
}

impl OperationCatalog {
    pub fn resolve(&self, name: &str) -> Option<&CatalogEntry> {
        self.entries.get(self.names.get(name)?)
    }

    pub fn entries(&self) -> impl Iterator<Item = &CatalogEntry> {
        self.entries.values()
    }

    pub fn children<'a>(&'a self, path: &str) -> BTreeSet<&'a str> {
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}.")
        };
        self.entries
            .values()
            .filter_map(|entry| entry.spec.path.strip_prefix(&prefix)?.split('.').next())
            .collect()
    }

    /// Deterministic schema fingerprint for the current catalog format.
    pub fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf29ce484222325_u64;
        for entry in self.entries.values() {
            let schema = ron::to_string(&(
                2u8,
                &entry.spec,
                &entry.bindings,
                &entry.aliases,
                entry.command,
                &entry.fixed_arguments,
                entry.legacy_option,
            ))
            .expect("operation descriptors contain only serializable metadata");
            for byte in schema.bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
            }
        }
        hash
    }

    fn conflict(
        &self,
        name: &str,
        incumbent: &OperationId,
        incoming: &CatalogEntry,
    ) -> CatalogError {
        let old = &self.entries[incumbent].spec;
        CatalogError::Collision {
            name: name.to_owned(),
            existing: (old.id.clone(), old.owner),
            incoming: (incoming.spec.id.clone(), incoming.spec.owner),
        }
    }

    fn insert(&mut self, entry: CatalogEntry) -> Result<(), CatalogError> {
        let id = &entry.spec.id;
        let path = &entry.spec.path;
        validate(path, false)?;
        if entry.bindings.is_empty() || id.0.is_empty() || id.0.len() > MAX_NAME {
            return Err(CatalogError::InvalidName(id.0.clone()));
        }
        let overlay = self.entries.get(id);
        if let Some(old) = overlay {
            let alias_conflict = entry.aliases.iter().any(|incoming| {
                old.aliases.iter().any(|existing| {
                    existing.name == incoming.name && existing.arguments != incoming.arguments
                })
            });
            if old.spec != entry.spec
                || !old.bindings.is_disjoint(&entry.bindings)
                || alias_conflict
            {
                return Err(self.conflict(path, id, &entry));
            }
        } else if self.entries.len() >= MAX_OPERATIONS {
            return Err(CatalogError::Capacity);
        }
        if entry.aliases.len() > MAX_ALIASES {
            return Err(CatalogError::Capacity);
        }
        let mut spellings = BTreeSet::new();
        for name in std::iter::once(path.as_str()).chain(entry.aliases.iter().map(|a| a.name)) {
            validate(name, name != path)?;
            if !spellings.insert(name) {
                return Err(CatalogError::Collision {
                    name: name.into(),
                    existing: (id.clone(), entry.spec.owner),
                    incoming: (id.clone(), entry.spec.owner),
                });
            }
            if let Some(existing) = self.names.get(name) {
                if existing != id {
                    return Err(self.conflict(name, existing, &entry));
                }
            }
            if let Some(existing) = self.namespaces.get(name) {
                return Err(self.conflict(name, existing, &entry));
            }
        }
        let aliases_after = self.names.len() - self.entries.len()
            + entry
                .aliases
                .iter()
                .filter(|a| !self.names.contains_key(a.name))
                .count();
        if aliases_after > MAX_ALIASES {
            return Err(CatalogError::Capacity);
        }
        for name in &spellings {
            for (dot, _) in name.match_indices('.') {
                let parent = &name[..dot];
                if let Some(existing) = self.names.get(parent) {
                    return Err(self.conflict(parent, existing, &entry));
                }
                if spellings.contains(parent) {
                    return Err(CatalogError::Collision {
                        name: parent.into(),
                        existing: (id.clone(), entry.spec.owner),
                        incoming: (id.clone(), entry.spec.owner),
                    });
                }
            }
        }
        for name in spellings {
            for (dot, _) in name.match_indices('.') {
                self.namespaces.insert(name[..dot].into(), id.clone());
            }
            self.names.insert(name.into(), id.clone());
        }
        if let Some(old) = self.entries.get_mut(id) {
            old.bindings.extend(entry.bindings);
            for alias in entry.aliases {
                if !old.aliases.iter().any(|a| a.name == alias.name) {
                    old.aliases.push(alias);
                }
            }
        } else {
            self.entries.insert(id.clone(), entry);
        }
        Ok(())
    }
}

fn validate(name: &str, alias: bool) -> Result<(), CatalogError> {
    let parts = name.split(['.', ' ']);
    let valid = name.len() <= MAX_NAME
        && parts.clone().count() <= MAX_DEPTH
        && (alias || !name.contains(' '))
        && parts.clone().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
        });
    if valid {
        Ok(())
    } else {
        Err(CatalogError::InvalidName(name.into()))
    }
}

#[derive(Default)]
pub struct CatalogBuilder(OperationCatalog);

impl CatalogBuilder {
    /// Validate a whole domain package before publishing any of its names.
    pub fn register(
        &mut self,
        entries: impl IntoIterator<Item = CatalogEntry>,
    ) -> Result<(), CatalogError> {
        let mut staged = self.0.clone();
        for entry in entries {
            staged.insert(entry)?;
        }
        self.0 = staged;
        Ok(())
    }

    pub fn finish(mut self) -> OperationCatalog {
        for entry in self.0.entries.values_mut() {
            entry.aliases.sort_by_key(|alias| alias.name);
        }
        self.0
    }
}
