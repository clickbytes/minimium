//! Administrative water status declaration and bounded owner-snapshot projection.
use crate::runtime::{WaterJobStatus, WaterStatus};
use minimium_commands::operation::{
    CatalogEntry, CommandValue as V, DomainBinding, Effect, OperationKind, ParameterSpec,
    PersistenceKind, ScalarType, TypeRef,
};
use std::collections::BTreeMap;

pub(crate) fn entry(name: &str) -> Option<CatalogEntry> {
    if name != "water status" {
        return None;
    }
    Some(
        DomainBinding {
            owner: "ServerRuntime/ExcavationState",
            scope: "WorldInstance",
            context: "explicit instance",
            permission: "ServerAdmin",
            capability: None,
            source: "OwnerSnapshot",
            persistence: PersistenceKind::Transient,
        }
        .declaration(
            "world.water.status",
            OperationKind::Method,
            &[ParameterSpec::required("instance_id", "WorldInstanceId")
                .scalar(ScalarType::NonzeroU128)],
            TypeRef::domain("WaterStatus"),
            None,
            Effect::Query,
        ),
    )
}

fn record(fields: impl IntoIterator<Item = (&'static str, V)>) -> V {
    V::Record(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn version(v: minimium_procgen::SectorContentVersion) -> V {
    record([
        ("edits", V::U64(v.edits)),
        ("remnants", V::U64(v.remnants)),
        ("volume", V::U64(v.volume)),
    ])
}

fn job(j: WaterJobStatus) -> V {
    record([
        ("state", V::Enum(j.state.label().into())),
        ("target_tick", V::U64(j.target_tick)),
        ("input_terrain_version", version(j.input_terrain_version)),
        ("input_fluid_revision", V::U64(j.input_fluid_revision)),
    ])
}

pub(crate) fn project(s: &WaterStatus) -> V {
    let digest = s
        .catalog_digest
        .map(|bytes| {
            let mut hex = String::with_capacity(64);
            for byte in bytes {
                use std::fmt::Write;
                let _ = write!(hex, "{byte:02x}");
            }
            V::Text(hex)
        })
        .unwrap_or(V::Unit);
    record([
        ("source", V::Enum("owner_snapshot".into())),
        (
            "consistency",
            V::Enum("owner_capture_with_pending_boundary".into()),
        ),
        ("instance_id", V::U128(s.instance.get())),
        ("capture_id", V::U64(s.capture_id)),
        ("committed_tick", V::U64(s.committed_tick)),
        (
            "pending_target_tick",
            s.pending_target_tick.map_or(V::Unit, V::U64),
        ),
        ("state", V::Enum(s.state.label().into())),
        ("reason", s.reason.map_or(V::Unit, |r| V::Enum(r.into()))),
        ("fluid_revision", s.fluid_revision.map_or(V::Unit, V::U64)),
        (
            "persisted_revision",
            s.persisted_revision.map_or(V::Unit, V::U64),
        ),
        ("dirty", s.dirty.map_or(V::Unit, V::Bool)),
        ("catalog_digest", digest),
        (
            "terrain_version",
            s.terrain_version.map_or(V::Unit, version),
        ),
        ("job", s.job.map_or(V::Unit, job)),
        ("failure", s.failure.map_or(V::Unit, |r| V::Enum(r.into()))),
        ("retry_after_ms", s.retry_after_ms.map_or(V::Unit, V::U64)),
        (
            "checkpoint_due_ms",
            s.checkpoint_due_ms.map_or(V::Unit, V::U64),
        ),
        (
            "active_cells",
            s.active_cells.map_or(V::Unit, |v| V::U64(v as u64)),
        ),
        (
            "reserved_limit",
            s.reserved_limit.map_or(V::Unit, |v| V::U64(v as u64)),
        ),
    ])
}

pub(crate) fn format(s: &WaterStatus) -> String {
    project(s).encode()
}
