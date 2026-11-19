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
        source: "OwnedAdminControl",
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
        "bolids status" => {
            binding.declaration(
                "server.sky.bolids.status",
                Method,
                &[ParameterSpec::required("shower_id", "BolidShowerId")
                    .scalar(ScalarType::NonzeroU64)],
                TypeRef::domain("BolidShowerStatus"),
                None,
                Query,
            )
        }
        "impacts status" => binding.declaration(
            "world.impacts.status",
            Method,
            &[
                ParameterSpec::required("instance_id", "WorldInstanceId")
                    .scalar(ScalarType::NonzeroU128),
                ParameterSpec::required("body_id", "ImpactBodyId").scalar(ScalarType::NonzeroU64),
            ],
            TypeRef::domain("ImpactReceipt"),
            None,
            Query,
        ),
        _ => return None,
    })
}
