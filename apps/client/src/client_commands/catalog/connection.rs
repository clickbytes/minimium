use minimium_commands::operation::ParameterSpec;
use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};

pub(super) fn entry(name: &str) -> Option<CatalogEntry> {
    let binding = DomainBinding {
        owner: "ConnectionController",
        scope: "Client",
        context: "connection attempt or session",
        permission: "Local",
        capability: None,
        source: "ClientHost",
        persistence: PersistenceKind::Transient,
    };
    Some(match name {
        "network stats" => binding.declaration(
            "client.connection.stats",
            Method,
            &[],
            TypeRef::domain("ConnectionStats"),
            None,
            Query,
        ),
        "console quit" => binding.declaration(
            "client.quit",
            Method,
            &[],
            TypeRef::domain("ShutdownRequestReceipt"),
            None,
            ProcessLifecycle,
        ),
        "network host local" => binding.declaration(
            "client.connection.host_local",
            Method,
            &[ParameterSpec::optional("threads", "LocalLanes", None)],
            TypeRef::domain("ConnectionAttempt"),
            None,
            NetworkLifecycle,
        ),
        "network connect" => binding.declaration(
            "client.connection.connect",
            Method,
            &[ParameterSpec::required("endpoint", "Endpoint")],
            TypeRef::domain("ConnectionAttempt"),
            None,
            NetworkLifecycle,
        ),
        "network disconnect" => binding.declaration(
            "client.connection.disconnect",
            Method,
            &[],
            TypeRef::domain("DisconnectReceipt"),
            None,
            NetworkLifecycle,
        ),
        "network ping" => binding.declaration(
            "client.connection.ping",
            Method,
            &[],
            TypeRef::domain("PingReceipt"),
            None,
            NetworkRequest,
        ),
        _ => return None,
    })
}
