use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};

pub(super) fn entry(name: &str) -> Option<CatalogEntry> {
    let binding = DomainBinding {
        owner: "WorldRegistry",
        scope: "PrimaryInstance",
        context: "primary manifest",
        permission: "ServerAdmin",
        capability: None,
        source: "OwnerSnapshot",
        persistence: PersistenceKind::WorldSnapshot,
    };
    Some(match name {
        "world.seed" => binding.declaration(
            "world.primary.generation.seed",
            Property,
            &[],
            TypeRef::domain("PropertyValue<GenerationSeed U128>"),
            None,
            Query,
        ),
        "world.generation_revision" => binding.declaration(
            "world.primary.generation.revision",
            Property,
            &[],
            TypeRef::domain("PropertyValue<GenerationRevision>"),
            None,
            Query,
        ),
        "world.generator_digest" => binding.declaration(
            "world.primary.generation.digest",
            Property,
            &[],
            TypeRef::domain("PropertyValue<Digest>"),
            Some("hex bytes"),
            Query,
        ),
        _ => return None,
    })
}
