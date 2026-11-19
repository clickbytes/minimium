use minimium_commands::operation::ParameterSpec;
use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};

pub(super) fn entry(name: &str) -> Option<CatalogEntry> {
    let binding = DomainBinding {
        owner: "ServerRuntime",
        scope: "Server",
        context: "owner lane",
        permission: "ServerAdmin",
        capability: None,
        source: "OwnerSnapshot",
        persistence: PersistenceKind::None,
    };
    Some(match name {
        "status" => binding.declaration(
            "server.status",
            Method,
            &[],
            TypeRef::domain("RuntimeStatus"),
            None,
            Query,
        ),
        "players list" => binding.declaration(
            "server.players.list",
            Method,
            &[
                ParameterSpec::optional("cursor", "Cursor", None),
                ParameterSpec::optional("limit", "U16", Some("64")),
            ],
            TypeRef::domain("Page<PlayerRow>"),
            None,
            Query,
        ),
        "metrics show" => binding.declaration(
            "server.metrics.status",
            Method,
            &[],
            TypeRef::domain("MetricsSnapshot"),
            None,
            Query,
        ),
        "stop" => binding.declaration(
            "server.stop",
            Method,
            &[],
            TypeRef::domain("ShutdownReceipt"),
            None,
            ProcessLifecycle,
        ),
        _ => return None,
    })
}
