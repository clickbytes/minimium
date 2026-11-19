//! Shared clock operation schemas for the client sample and server owner binding.
use minimium_commands::operation::ParameterSpec;
use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};

#[derive(Clone, Copy)]
pub enum ClockReadBinding {
    SessionSample,
    OwnerSnapshot,
}

pub fn clock_entry(name: &str, source: ClockReadBinding) -> Option<CatalogEntry> {
    let binding = DomainBinding {
        owner: "EnvironmentClock",
        scope: "SharedSimulation",
        context: "simulation",
        permission: "ServerAdmin",
        capability: None,
        source: match source {
            ClockReadBinding::SessionSample => "SessionSample",
            ClockReadBinding::OwnerSnapshot => "OwnerSnapshot",
        },
        persistence: PersistenceKind::WorldSnapshot,
    };
    let mut entry = match name {
        "world.time.mode" | "time mode real_time" | "time mode time_scale" => binding.declaration(
            "world.time.mode",
            Property,
            &[ParameterSpec::required("value", "ClockMode")],
            TypeRef::domain("PropertyValue<ClockMode>"),
            None,
            AuthoritativeMutation,
        ),
        "world.time.scale" | "time scale" => binding.declaration(
            "world.time.scale",
            Property,
            &[ParameterSpec::required("value", "TimeScalePpm")],
            TypeRef::domain("PropertyValue<TimeScalePpm>"),
            Some("x; exact ppm"),
            AuthoritativeMutation,
        ),
        "world.time.offline_policy" | "time offline pause" | "time offline advance" => binding
            .declaration(
                "world.time.offline_policy",
                Property,
                &[ParameterSpec::required("value", "OfflinePolicy")],
                TypeRef::domain("PropertyValue<OfflinePolicy>"),
                None,
                AuthoritativeMutation,
            ),
        "world.time.utc" => binding.declaration(
            "world.time.utc",
            Property,
            &[],
            TypeRef::domain("PropertyValue<WorldTimeMs>"),
            Some("UTC milliseconds (exact WorldTimeMs); presentation by host formatter"),
            Query,
        ),
        "world.time.revision" => binding.declaration(
            "world.time.revision",
            Property,
            &[],
            TypeRef::domain("PropertyValue<ClockRevision>"),
            None,
            Query,
        ),
        "set_time" => binding.declaration(
            "world.time.set_utc_hhmm",
            Method,
            &[ParameterSpec::required("hhmm", "UtcHourMinute")],
            TypeRef::domain("ClockMutationReceipt"),
            Some("UTC HHMM"),
            AuthoritativeMutation,
        ),
        "set_time_local" => binding.declaration(
            "world.time.set_solar_hhmm",
            Method,
            &[ParameterSpec::required("hhmm", "SolarHourMinute")],
            TypeRef::domain("ClockMutationReceipt"),
            Some("solar HHMM"),
            AuthoritativeMutation,
        ),
        "time status" => binding.declaration(
            "world.time.status",
            Method,
            &[],
            TypeRef::domain("TimeStatus"),
            None,
            Query,
        ),
        "time set" => binding.declaration(
            "world.time.set",
            Method,
            &[ParameterSpec::required("utc", "WorldTimeMs")],
            TypeRef::domain("ClockMutationReceipt"),
            None,
            AuthoritativeMutation,
        ),
        _ => return None,
    };
    if let Some(read) = &mut entry.spec.read {
        read.permission = "SessionSample or ServerAdmin";
    }
    if let Some(invoke) = &mut entry.spec.invoke {
        if invoke.effect == Query {
            invoke.permission = "SessionSample or ServerAdmin";
        }
    }
    Some(entry)
}
