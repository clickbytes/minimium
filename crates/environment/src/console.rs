//! Shared clock operation schemas for the client sample and server owner binding.
use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};
use minimium_commands::operation::{ParameterSpec, ScalarType};

pub mod status;
pub mod values;

pub fn live_clock_field(key: &str) -> bool {
    matches!(
        key,
        "world.time.mode"
            | "world.time.scale"
            | "world.time.offline_policy"
            | "world.time.utc"
            | "world.time.revision"
            | "world.time.achieved_scale"
    )
}

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
    if name == "world.time.initial_config" {
        let mut entry = DomainBinding {
            owner: "ServerRuntime",
            scope: "Server",
            context: "resolved startup configuration",
            source: "OwnerSnapshot",
            persistence: PersistenceKind::ServerConfig,
            ..binding
        }
        .declaration(
            "world.time.initial_config",
            Property,
            &[],
            TypeRef::domain("ConfigurationValue<Path>"),
            None,
            Query,
        );
        entry.spec.lifecycle = minimium_commands::PropertyLifecycle::CreationOnly;
        return Some(entry);
    }
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
            &[ParameterSpec::required("value", "TimeScalePpm").scalar(ScalarType::Decimal)],
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
        "world.time.achieved_scale" => binding.declaration(
            "world.time.achieved_scale",
            Property,
            &[],
            TypeRef::domain("PropertyValue<MeasuredRatePpm>"),
            Some("exact observed ppm; display x"),
            Query,
        ),
        "world.time.diagnostics" => binding.declaration(
            "world.time.diagnostics",
            Method,
            &[],
            TypeRef::domain("TimeDiagnostics"),
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
            &[ParameterSpec::required("utc", "WorldTimeMs").scalar(ScalarType::InstantMs)],
            TypeRef::domain("ClockMutationReceipt"),
            None,
            AuthoritativeMutation,
        ),
        _ => return None,
    };
    if name == "set_time_local" {
        entry.spec.context = "actor and expected active WorldContext";
    }
    if let Some(read) = &mut entry.spec.read {
        read.permission = "SessionSample or ServerAdmin";
    }
    if let Some(invoke) = &mut entry.spec.invoke {
        if invoke.effect == Query {
            invoke.permission = "SessionSample or ServerAdmin";
        }
    }
    if matches!(
        name,
        "time mode real_time"
            | "time mode time_scale"
            | "time offline pause"
            | "time offline advance"
    ) {
        let (alias, value) = match name {
            "time mode real_time" => ("time mode real_time", "real_time"),
            "time mode time_scale" => ("time mode time_scale", "time_scale"),
            "time offline pause" => ("time offline pause", "pause"),
            _ => ("time offline advance", "advance"),
        };
        entry.fixed_arguments.insert(alias, value);
    }
    Some(entry)
}

/// Register new domain-owned methods once in each host catalog.
pub fn register_operations(
    package: &mut minimium_commands::operation::LegacyPackage,
    source: ClockReadBinding,
) -> Result<(), minimium_commands::operation::CatalogError> {
    package.operation(
        clock_entry("world.time.diagnostics", source).expect("environment diagnostics declaration"),
    )
}
