use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};

pub(super) fn entry(name: &str) -> Option<CatalogEntry> {
    let binding = DomainBinding {
        owner: "ServerRuntime",
        scope: "AllWorlds",
        context: "save boundary",
        permission: "ServerAdmin",
        capability: None,
        source: "OwnerSnapshot",
        persistence: PersistenceKind::WorldSnapshot,
    };
    Some(match name {
        "world save" => binding.declaration(
            "server.persistence.save",
            Method,
            &[],
            TypeRef::domain("SaveJobReceipt"),
            None,
            PersistenceJob,
        ),
        _ => return None,
    })
}
