use minimium_commands::operation::ParameterSpec;
use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};

pub(super) fn entry(name: &str) -> Option<CatalogEntry> {
    let binding = DomainBinding {
        owner: "OperationCatalog",
        scope: "Host",
        context: "host catalog",
        permission: "Local",
        capability: None,
        source: "ClientHost",
        persistence: PersistenceKind::None,
    };
    Some(match name {
        "help" => binding.declaration(
            "ui.console.help",
            Method,
            &[ParameterSpec::optional("target", "Name", None)],
            TypeRef::domain("CatalogHelp"),
            None,
            Query,
        ),
        "console clear" => binding.declaration(
            "ui.console.clear",
            Method,
            &[],
            TypeRef::domain("Unit"),
            None,
            LocalPresentation,
        ),
        _ => return None,
    })
}
