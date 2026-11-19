use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};
use minimium_commands::operation::{ParameterSpec, ScalarType};

pub(super) fn entry(name: &str) -> Option<CatalogEntry> {
    let mut binding = DomainBinding {
        owner: "MovementPolicy",
        scope: "SessionActor",
        context: "current WorldContext",
        permission: "Gameplay",
        capability: None,
        source: "SessionSample",
        persistence: PersistenceKind::Transient,
    };
    let mut entry = match name {
        "player.movement.fly" | "movement fly" => {
            binding.permission = "DeveloperFly";
            binding.declaration(
                "player.movement.fly",
                Property,
                &[ParameterSpec::required("value", "Bool").scalar(ScalarType::Bool)],
                TypeRef::domain("PropertyValue<Bool>"),
                None,
                PlayerIntent,
            )
        }
        "player.movement.speed" | "movement speed" => {
            binding.permission = "DeveloperMovementPolicy";
            binding.declaration(
                "player.movement.speed",
                Property,
                &[ParameterSpec::required("value", "FiniteF32").scalar(ScalarType::FiniteF32)],
                TypeRef::domain("PropertyValue<FiniteF32>"),
                Some("m/s"),
                PlayerIntent,
            )
        }
        "player.movement.accel" | "movement accel" => {
            binding.permission = "DeveloperMovementPolicy";
            binding.declaration(
                "player.movement.accel",
                Property,
                &[ParameterSpec::required("value", "FiniteF32").scalar(ScalarType::FiniteF32)],
                TypeRef::domain("PropertyValue<FiniteF32>"),
                Some("m/s²"),
                PlayerIntent,
            )
        }
        "movement move" => binding.declaration(
            "player.movement.move",
            Method,
            &[
                ParameterSpec::required("x", "FiniteF32").scalar(ScalarType::FiniteF32),
                ParameterSpec::required("z", "FiniteF32").scalar(ScalarType::FiniteF32),
            ],
            TypeRef::domain("MovementReceipt"),
            Some("dimensionless tangent direction -1..1"),
            PlayerIntent,
        ),
        "movement jump" => binding.declaration(
            "player.movement.jump",
            Method,
            &[],
            TypeRef::domain("MovementReceipt"),
            None,
            PlayerIntent,
        ),
        "set_altitude" => {
            binding.permission = "DeveloperTeleport";
            binding.context = "current WorldContext with flight enabled";
            binding.declaration(
                "player.movement.set_altitude",
                Method,
                &[ParameterSpec::required("km", "FiniteF32").scalar(ScalarType::FiniteF32)],
                TypeRef::domain("AltitudeReceipt"),
                Some("km"),
                PlayerIntent,
            )
        }
        "dev goto" => {
            binding.permission = "DeveloperTeleport";
            binding.declaration(
                "player.movement.goto",
                Method,
                &[
                    ParameterSpec::required("latitude", "FiniteF32").scalar(ScalarType::FiniteF32),
                    ParameterSpec::required("longitude", "FiniteF32").scalar(ScalarType::FiniteF32),
                ],
                TypeRef::domain("TeleportReceipt"),
                Some("degrees"),
                PlayerIntent,
            )
        }
        _ => return None,
    };
    if let Some(read) = &mut entry.spec.read {
        read.permission = "OwnSession";
    }
    Some(entry)
}
