use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};
use minimium_commands::operation::{ParameterSpec, ScalarType};

pub(super) fn entry(name: &str, persistent: bool) -> Option<CatalogEntry> {
    let mut binding = DomainBinding {
        owner: "DesktopHost",
        scope: "Client",
        context: "local presentation",
        permission: "Local",
        capability: None,
        source: "ClientHost",
        persistence: if persistent {
            PersistenceKind::ClientConfig
        } else {
            PersistenceKind::Transient
        },
    };
    Some(match name {
        "client.sky.meteors.enabled" => binding.declaration(
            "client.sky.meteors.enabled",
            Property,
            &[ParameterSpec::required("value", "Bool").scalar(ScalarType::Bool)],
            TypeRef::domain("PropertyValue<Bool>"),
            None,
            LocalPresentation,
        ),
        "client.sky.meteors.rate" => binding.declaration(
            "client.sky.meteors.rate",
            Property,
            &[ParameterSpec::required("value", "FiniteF64").scalar(ScalarType::FiniteF64)],
            TypeRef::domain("PropertyValue<FiniteF64>"),
            Some("multiplier x"),
            LocalPresentation,
        ),
        "client.sky.meteors.quality" => binding.declaration(
            "client.sky.meteors.quality",
            Property,
            &[ParameterSpec::required("value", "SkyQuality")],
            TypeRef::domain("PropertyValue<SkyQuality>"),
            None,
            LocalPresentation,
        ),
        "client.sky.meteors.seed_override" | "sky meteors seed" => binding.declaration(
            "client.sky.meteors.seed_override",
            Property,
            &[ParameterSpec::required("value", "Option<U64>").scalar(ScalarType::OptionalU64)],
            TypeRef::domain("PropertyValue<Option<U64>>"),
            None,
            LocalPresentation,
        ),
        "client.sky.meteors.sound" => binding.declaration(
            "client.sky.meteors.sound",
            Property,
            &[ParameterSpec::required("value", "Bool").scalar(ScalarType::Bool)],
            TypeRef::domain("PropertyValue<Bool>"),
            None,
            LocalPresentation,
        ),
        "client.sky.names" | "sky names" => binding.declaration(
            "client.sky.names",
            Property,
            &[ParameterSpec::required("value", "Bool").scalar(ScalarType::Bool)],
            TypeRef::domain("PropertyValue<Bool>"),
            None,
            LocalPresentation,
        ),
        "client.weather.cloud_detail" | "weather clouds detail" => binding.declaration(
            "client.weather.cloud_detail",
            Property,
            &[ParameterSpec::required("value", "U8").scalar(ScalarType::U8)],
            TypeRef::domain("PropertyValue<U8>"),
            Some("depth"),
            LocalPresentation,
        ),
        "client.sky.demo" | "sky demo" => binding.declaration(
            "client.sky.demo",
            Property,
            &[ParameterSpec::required("value", "Bool").scalar(ScalarType::Bool)],
            TypeRef::domain("PropertyValue<Bool>"),
            None,
            LocalPresentation,
        ),
        "diagnostics fps" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.render.frames.status",
                Method,
                &[],
                TypeRef::domain("FrameStats"),
                None,
                Query,
            )
        }
        "world teleport" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "ui.map.teleport.arm",
                Method,
                &[],
                TypeRef::domain("MapSelectionReceipt"),
                None,
                LocalPresentation,
            )
        }
        "sky meteors debug" => binding.declaration(
            "client.sky.meteors.debug.mode",
            Property,
            &[ParameterSpec::required("value", "MeteorDebug")],
            TypeRef::domain("PropertyValue<MeteorDebug>"),
            None,
            LocalPresentation,
        ),
        "sky info" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.sky.status",
                Method,
                &[],
                TypeRef::domain("SkyStatus"),
                None,
                Query,
            )
        }
        "sky bodies" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.sky.bodies.list",
                Method,
                &[],
                TypeRef::domain("BoundedPage<ObservedBody>"),
                None,
                Query,
            )
        }
        "sky meteors status" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.sky.meteors.status",
                Method,
                &[],
                TypeRef::domain("MeteorStatus"),
                None,
                Query,
            )
        }
        "sky meteors spawn" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.sky.meteors.spawn",
                Method,
                &[
                    ParameterSpec::required("preset", "MeteorPreset"),
                    ParameterSpec::optional("seed", "U64", None).scalar(ScalarType::U64),
                ],
                TypeRef::domain("LocalMeteorReceipt"),
                None,
                LocalPresentation,
            )
        }
        "sky meteors debug light" => binding.declaration(
            "client.sky.meteors.debug.light",
            Property,
            &[ParameterSpec::required("value", "MeteorLightDebug")],
            TypeRef::domain("PropertyValue<MeteorLightDebug>"),
            None,
            LocalPresentation,
        ),
        "sky meteors clear" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.sky.meteors.clear",
                Method,
                &[],
                TypeRef::domain("ClearedCount"),
                None,
                LocalPresentation,
            )
        }
        "terrain status" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.render.terrain.status",
                Method,
                &[],
                TypeRef::domain("TerrainPresentationStatus"),
                None,
                Query,
            )
        }
        "atmosphere status" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.render.atmosphere.status",
                Method,
                &[],
                TypeRef::domain("AtmospherePresentationStatus"),
                None,
                Query,
            )
        }
        "weather status" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.weather.status",
                Method,
                &[],
                TypeRef::domain("WeatherPresentationStatus"),
                None,
                Query,
            )
        }
        "dev clouds" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.weather.clouds.targets",
                Method,
                &[],
                TypeRef::domain("CloudTargets"),
                None,
                Query,
            )
        }
        "dev bolid transmission" => binding.declaration(
            "client.render.bolids.transmission",
            Property,
            &[ParameterSpec::required("value", "Bool").scalar(ScalarType::Bool)],
            TypeRef::domain("PropertyValue<Bool>"),
            None,
            LocalPresentation,
        ),
        "dev bolid path" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.render.bolids.restore_path",
                Method,
                &[
                    ParameterSpec::required("id", "U64").scalar(ScalarType::U64),
                    ParameterSpec::required("kind", "U8").scalar(ScalarType::U8),
                    ParameterSpec::required("seed", "U64").scalar(ScalarType::U64),
                    ParameterSpec::required("duration_ms", "U16").scalar(ScalarType::U16),
                    ParameterSpec::required("x0", "FiniteF32").scalar(ScalarType::FiniteF32),
                    ParameterSpec::required("y0", "FiniteF32").scalar(ScalarType::FiniteF32),
                    ParameterSpec::required("z0", "FiniteF32").scalar(ScalarType::FiniteF32),
                    ParameterSpec::required("x1", "FiniteF32").scalar(ScalarType::FiniteF32),
                    ParameterSpec::required("y1", "FiniteF32").scalar(ScalarType::FiniteF32),
                    ParameterSpec::required("z1", "FiniteF32").scalar(ScalarType::FiniteF32),
                ],
                TypeRef::domain("PreviewReceipt"),
                Some("duration ms; world positions m"),
                LocalPresentation,
            )
        }
        "dev bolid fragments" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.render.bolids.fragments",
                Method,
                &[],
                TypeRef::domain("FragmentReport"),
                None,
                Query,
            )
        }
        "dev bolid lighting" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.render.bolids.lighting",
                Method,
                &[],
                TypeRef::domain("LightingReport"),
                None,
                Query,
            )
        }
        "dev bolid status" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.render.bolids.status",
                Method,
                &[],
                TypeRef::domain("RetainedBolidReport"),
                None,
                Query,
            )
        }
        "dev bolid replay" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.render.bolids.replay",
                Method,
                &[
                    ParameterSpec::required("age_seconds", "FiniteF32")
                        .scalar(ScalarType::FiniteF32),
                    ParameterSpec::optional("seed", "U64", None).scalar(ScalarType::U64),
                    ParameterSpec::optional("event_id", "U64", None).scalar(ScalarType::U64),
                ],
                TypeRef::domain("PreviewReceipt"),
                Some("simulation/presentation seconds; exact U64 IDs/seeds"),
                LocalPresentation,
            )
        }
        "dev bolid freeze" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.render.bolids.freeze",
                Method,
                &[
                    ParameterSpec::required("age_seconds", "FiniteF32")
                        .scalar(ScalarType::FiniteF32),
                    ParameterSpec::optional("seed", "U64", None).scalar(ScalarType::U64),
                    ParameterSpec::optional("event_id", "U64", None).scalar(ScalarType::U64),
                ],
                TypeRef::domain("PreviewReceipt"),
                Some("simulation/presentation seconds; exact U64 IDs/seeds"),
                LocalPresentation,
            )
        }
        _ => return None,
    })
}
