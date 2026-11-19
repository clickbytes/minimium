use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};
use minimium_commands::operation::{ParameterSpec, ScalarType};

pub(super) fn entry(name: &str) -> Option<CatalogEntry> {
    let binding = DomainBinding {
        owner: "ServerRuntime",
        scope: "Server",
        context: "authorized target instance",
        permission: "ServerAdmin",
        capability: None,
        source: "OwnerSnapshot",
        persistence: PersistenceKind::Transient,
    };
    Some(match name {
        "spawn_random_meteorid" => binding.declaration(
            "world.impacts.spawn_random",
            Method,
            &[],
            TypeRef::domain("ImpactReceipt"),
            None,
            AuthoritativeMutation,
        ),
        "spawn_random_bolid" => binding.declaration(
            "server.sky.bolids.spawn",
            Method,
            &[ParameterSpec::optional("count", "U16", Some("1")).scalar(ScalarType::U16)],
            TypeRef::domain("BolidShowerReceipt"),
            Some("count of bolids"),
            AuthoritativeMutation,
        ),
        _ => return None,
    })
}
