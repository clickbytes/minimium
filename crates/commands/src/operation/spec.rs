use crate::{CommandArgumentSpec, PropertyLifecycle, PropertySpec};
use std::collections::BTreeSet;

#[derive(serde::Serialize, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct OperationId(pub String);

#[derive(serde::Serialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationKind {
    Method,
    Property,
}

#[derive(serde::Serialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum PersistenceKind {
    ClientConfig,
    ServerConfig,
    WorldSnapshot,
    Transient,
    None,
}

#[derive(serde::Serialize, Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParameterSpec {
    pub name: &'static str,
    pub value_type: TypeRef,
    pub optional: bool,
    pub default: Option<&'static str>,
}

impl ParameterSpec {
    pub const fn required(name: &'static str, schema: &'static str) -> Self {
        Self {
            name,
            value_type: TypeRef(schema),
            optional: false,
            default: None,
        }
    }

    pub const fn optional(
        name: &'static str,
        schema: &'static str,
        default: Option<&'static str>,
    ) -> Self {
        Self {
            name,
            value_type: TypeRef(schema),
            optional: true,
            default,
        }
    }
}

#[derive(serde::Serialize, Clone, Copy, Debug, Eq, PartialEq)]
pub struct TypeRef(pub &'static str);

impl TypeRef {
    pub const fn domain(schema: &'static str) -> Self {
        Self(schema)
    }
}

#[derive(serde::Serialize, Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    Query,
    LocalPresentation,
    PlayerIntent,
    AuthoritativeMutation,
    PersistenceJob,
    NetworkRequest,
    NetworkLifecycle,
    ProcessLifecycle,
    FilesystemWrite,
    /// The registered extension executes its own handler and declares no effect schema.
    LegacyModule,
}

#[derive(serde::Serialize, Clone, Copy, Debug, Eq, PartialEq)]
pub struct Access {
    pub effect: Effect,
    pub permission: &'static str,
}

#[derive(serde::Serialize, Clone, Debug, Eq, PartialEq)]
pub struct OperationSpec {
    pub id: OperationId,
    pub path: String,
    pub kind: OperationKind,
    pub arguments: Vec<ParameterSpec>,
    pub result: TypeRef,
    pub unit: Option<&'static str>,
    pub owner: &'static str,
    pub scope: &'static str,
    pub context: &'static str,
    pub capability: Option<&'static str>,
    pub read: Option<Access>,
    pub write: Option<Access>,
    pub invoke: Option<Access>,
    pub lifecycle: PropertyLifecycle,
    pub persistence: PersistenceKind,
    pub help_key: String,
    pub scriptable: bool,
}

#[derive(serde::Serialize, Clone, Debug, Eq, PartialEq)]
pub struct AliasSpec {
    pub name: &'static str,
    pub usage: &'static str,
    pub arguments: &'static [CommandArgumentSpec],
}

#[derive(Clone, Debug)]
pub struct CatalogEntry {
    pub spec: OperationSpec,
    pub aliases: Vec<AliasSpec>,
    pub bindings: BTreeSet<&'static str>,
    pub property: Option<&'static PropertySpec>,
}

impl CatalogEntry {
    pub fn new(spec: OperationSpec, binding: &'static str) -> Self {
        Self {
            spec,
            aliases: Vec::new(),
            bindings: [binding].into(),
            property: None,
        }
    }

    pub fn describe(&self) -> String {
        use std::fmt::Write;
        let s = &self.spec;
        let signature = s
            .arguments
            .iter()
            .map(|p| {
                let mark = if p.optional { "?" } else { "" };
                match p.default {
                    Some(value) => format!("{}{mark}: {} = {value}", p.name, p.value_type.0),
                    None => format!("{}{mark}: {}", p.name, p.value_type.0),
                }
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mut output = format!("{} {:?} ({}) -> {}", s.path, s.kind, signature, s.result.0);
        for (key, value) in [("unit", s.unit), ("capability", s.capability)] {
            if let Some(value) = value {
                let _ = write!(output, " {key}={value}");
            }
        }
        let _ = write!(
            output,
            "\nowner={} scope={} context={} lifecycle={:?} persistence={:?}",
            s.owner, s.scope, s.context, s.lifecycle, s.persistence
        );
        for (key, access) in [("read", s.read), ("write", s.write), ("invoke", s.invoke)] {
            if let Some(access) = access {
                let _ = write!(
                    output,
                    "\n{key}: {:?} ({})",
                    access.effect, access.permission
                );
            }
        }
        let sources = self.bindings.iter().copied().collect::<Vec<_>>().join(", ");
        let _ = write!(
            output,
            "\nsource={sources} backend=legacy scriptable={}",
            s.scriptable
        );
        for alias in &self.aliases {
            let _ = write!(output, "\n  {}: ", alias.name);
            let _ = crate::write_command_help(&mut output, alias.usage, alias.arguments);
        }
        output
    }
}
