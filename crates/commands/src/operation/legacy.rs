//! Adapt registered legacy spellings without moving parsing or execution.
use super::*;
use crate::{CommandArgumentSpec, PropertyLifecycle, PropertySpec};
use std::collections::BTreeMap;

#[derive(Clone, Copy)]
pub struct DomainBinding {
    pub owner: &'static str,
    pub scope: &'static str,
    pub context: &'static str,
    pub permission: &'static str,
    pub capability: Option<&'static str>,
    pub source: &'static str,
    pub persistence: PersistenceKind,
}

impl DomainBinding {
    pub fn declaration(
        self,
        path: &str,
        kind: OperationKind,
        arguments: &[ParameterSpec],
        result: TypeRef,
        unit: Option<&'static str>,
        effect: Effect,
    ) -> CatalogEntry {
        let access = Access {
            effect,
            permission: self.permission,
        };
        let spec = OperationSpec {
            id: OperationId(path.to_owned()),
            path: path.to_owned(),
            kind,
            arguments: arguments.to_vec(),
            result,
            unit,
            owner: self.owner,
            scope: self.scope,
            context: self.context,
            capability: self.capability,
            read: (kind == OperationKind::Property).then_some(Access {
                effect: Effect::Query,
                permission: self.permission,
            }),
            write: (kind == OperationKind::Property && effect != Effect::Query).then_some(access),
            invoke: (kind == OperationKind::Method).then_some(access),
            lifecycle: if kind == OperationKind::Property && effect == Effect::Query {
                PropertyLifecycle::ReadOnly
            } else {
                PropertyLifecycle::Live
            },
            persistence: self.persistence,
            help_key: path.to_owned(),
            scriptable: false,
        };
        CatalogEntry::new(spec, self.source)
    }
}

/// Groups several registered legacy invocations of a single operation before
/// the complete package is submitted for transactional name validation.
#[derive(Default)]
pub struct LegacyPackage(BTreeMap<String, CatalogEntry>);

impl LegacyPackage {
    pub fn property(
        &mut self,
        mut entry: CatalogEntry,
        spec: &'static PropertySpec,
    ) -> Result<(), CatalogError> {
        entry.property = Some(spec);
        entry.spec.lifecycle = spec.lifecycle;
        let path = entry.spec.path.clone();
        if let Some(old) = self.0.get(&path) {
            return Err(CatalogError::Collision {
                name: path,
                existing: (old.spec.id.clone(), old.spec.owner),
                incoming: (entry.spec.id, entry.spec.owner),
            });
        }
        if spec.key != path {
            entry.aliases.push(AliasSpec {
                name: spec.key,
                usage: spec.key,
                arguments: &[],
            });
        }
        self.0.insert(path, entry);
        Ok(())
    }

    pub fn alias(
        &mut self,
        path: &str,
        name: &'static str,
        usage: &'static str,
        arguments: &'static [CommandArgumentSpec],
    ) -> Result<(), CatalogError> {
        let entry = self
            .0
            .get_mut(path)
            .ok_or_else(|| CatalogError::InvalidName(path.into()))?;
        if name != path {
            entry.aliases.push(AliasSpec {
                name,
                usage,
                arguments,
            });
        }
        Ok(())
    }

    pub fn command(
        &mut self,
        mut entry: CatalogEntry,
        name: &'static str,
        aliases: impl IntoIterator<Item = &'static str>,
        usage: &'static str,
        arguments: &'static [CommandArgumentSpec],
    ) -> Result<(), CatalogError> {
        let path = entry.spec.path.clone();
        if let Some(existing) = self.0.get(&path) {
            // Persistence and lifecycle come from the registered property; the
            // command only contributes a spelling with its own argument codec.
            entry.spec.persistence = existing.spec.persistence;
            entry.spec.lifecycle = existing.spec.lifecycle;
            if existing.property.is_none()
                || existing.spec != entry.spec
                || existing.bindings != entry.bindings
            {
                return Err(CatalogError::Collision {
                    name: path,
                    existing: (existing.spec.id.clone(), existing.spec.owner),
                    incoming: (entry.spec.id, entry.spec.owner),
                });
            }
        } else {
            self.0.insert(path.clone(), entry);
        }
        for spelling in std::iter::once(name).chain(aliases) {
            self.alias(&path, spelling, usage, arguments)?;
        }
        Ok(())
    }

    pub fn finish(self) -> Result<OperationCatalog, CatalogError> {
        let mut builder = CatalogBuilder::default();
        builder.register(self.0.into_values())?;
        Ok(builder.finish())
    }
}

impl From<&CommandArgumentSpec> for ParameterSpec {
    fn from(arg: &CommandArgumentSpec) -> Self {
        use crate::CommandArgumentType::*;
        let schema = match arg.value_type {
            Text => "String",
            Boolean => "Bool",
            Integer => "Integer",
            Decimal => "Decimal",
            Enumeration => "Enum",
            Path => "Path",
            Rfc3339Utc => "WorldTimeMs",
        };
        Self {
            name: arg.name,
            value_type: TypeRef(schema),
            optional: arg.optional,
            default: None,
        }
    }
}
