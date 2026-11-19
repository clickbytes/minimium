use minimium_commands::operation::ParameterSpec;
use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};

pub(super) fn entry(name: &str) -> Option<CatalogEntry> {
    let binding = DomainBinding {
        owner: "WorldRegistry",
        scope: "WorldRegistry",
        context: "explicit instance or registry page",
        permission: "ServerAdmin",
        capability: None,
        source: "OwnerSnapshot",
        persistence: PersistenceKind::WorldSnapshot,
    };
    Some(match name {
        "world list" => binding.declaration(
            "world.instances.list",
            Method,
            &[
                ParameterSpec::optional("cursor", "Cursor", None),
                ParameterSpec::optional("limit", "U16", Some("64")),
            ],
            TypeRef::domain("Page<WorldRow>"),
            None,
            Query,
        ),
        "world inspect" => binding.declaration(
            "world.instances.inspect",
            Method,
            &[ParameterSpec::required("instance_id", "WorldInstanceId")],
            TypeRef::domain("WorldInspect"),
            None,
            Query,
        ),
        "entity list" => binding.declaration(
            "world.objects.list",
            Method,
            &[
                ParameterSpec::optional("cursor", "Cursor", None),
                ParameterSpec::optional("limit", "U16", Some("64")),
            ],
            TypeRef::domain("Page<EntityOrPlayerRow>"),
            None,
            Query,
        ),
        "entity inspect" => binding.declaration(
            "world.entities.inspect",
            Method,
            &[
                ParameterSpec::required("instance_id", "WorldInstanceId"),
                ParameterSpec::required("entity_id", "EntityId"),
                ParameterSpec::optional("cursor", "Cursor", None),
                ParameterSpec::optional("limit", "U16", Some("64")),
            ],
            TypeRef::domain("Page<ComponentMetadata>"),
            None,
            Query,
        ),
        "entity despawn" => binding.declaration(
            "world.entities.despawn",
            Method,
            &[
                ParameterSpec::required("instance_id", "WorldInstanceId"),
                ParameterSpec::required("entity_id", "EntityId"),
            ],
            TypeRef::domain("EffectReceipt"),
            None,
            AuthoritativeMutation,
        ),
        _ => return None,
    })
}
