use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};
use minimium_commands::operation::{ParameterSpec, ScalarType};

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
        "server.persistence.status" => {
            binding.declaration(
                "server.persistence.status",
                Method,
                &[ParameterSpec::optional("job_id", "SaveJobId", None)
                    .scalar(ScalarType::NonzeroU128)],
                TypeRef::domain("SaveStatus"),
                None,
                Query,
            )
        }
        _ => return None,
    })
}
