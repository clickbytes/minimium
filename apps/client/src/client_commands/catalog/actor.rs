use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};
use minimium_commands::operation::{ParameterSpec, ScalarType};

pub(super) fn entry(name: &str) -> Option<CatalogEntry> {
    let mut binding = DomainBinding {
        owner: "ServerRuntime",
        scope: "SessionActor",
        context: "current WorldContext",
        permission: "Gameplay",
        capability: None,
        source: "SessionSample",
        persistence: PersistenceKind::Transient,
    };
    Some(match name {
        "diagnostics state" => binding.declaration(
            "player.state",
            Method,
            &[],
            TypeRef::domain("PlayerStateView"),
            None,
            Query,
        ),
        "dev inventory tools" => {
            binding.permission = "DeveloperTeleport";
            binding.declaration(
                "player.inventory.grant_tools",
                Method,
                &[],
                TypeRef::domain("InventoryOperationReceipt"),
                None,
                PlayerIntent,
            )
        }
        "world portal" => binding.declaration(
            "player.interaction.portal",
            Method,
            &[],
            TypeRef::domain("TransferReceipt"),
            None,
            PlayerIntent,
        ),
        "combat fire" => binding.declaration(
            "player.combat.fire",
            Method,
            &[],
            TypeRef::domain("FireReceipt"),
            None,
            PlayerIntent,
        ),
        "social say" => binding.declaration(
            "player.chat.send",
            Method,
            &[ParameterSpec::required("text", "String").scalar(ScalarType::Text)],
            TypeRef::domain("ChatReceipt"),
            None,
            PlayerIntent,
        ),
        "social name" => binding.declaration(
            "player.identity.set_name",
            Method,
            &[ParameterSpec::required("name", "PlayerName").scalar(ScalarType::Text)],
            TypeRef::domain("NameReceipt"),
            Some("1..24 ASCII bytes; [A-Za-z0-9_]"),
            PlayerIntent,
        ),
        _ => return None,
    })
}
