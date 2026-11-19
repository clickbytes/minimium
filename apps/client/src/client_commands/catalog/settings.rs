use minimium_commands::operation::ParameterSpec;
use minimium_commands::operation::{
    CatalogEntry, DomainBinding, Effect::*, OperationKind::*, PersistenceKind, TypeRef,
};

pub(super) fn entry(name: &str, persistent: bool) -> Option<CatalogEntry> {
    let mut binding = DomainBinding {
        owner: "SettingsStore",
        scope: "Client",
        context: "local preferences",
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
        "client.audio.meteor_volume" => binding.declaration(
            "client.audio.meteor_volume",
            Property,
            &[ParameterSpec::required("value", "FiniteF32")],
            TypeRef::domain("PropertyValue<FiniteF32>"),
            Some("gain 0..1"),
            LocalPresentation,
        ),
        "ui.locale" => binding.declaration(
            "ui.locale",
            Property,
            &[ParameterSpec::required("value", "Locale")],
            TypeRef::domain("PropertyValue<Locale>"),
            None,
            LocalPresentation,
        ),
        "client.camera.fov" | "camera fov" => {
            binding.capability = Some("RendererEffectiveValue");
            binding.declaration(
                "client.camera.fov",
                Property,
                &[ParameterSpec::required("value", "FiniteF32")],
                TypeRef::domain("PropertyValue<FiniteF32>"),
                Some("deg vertical"),
                LocalPresentation,
            )
        }
        "client.input.mouse_sensitivity" => binding.declaration(
            "client.input.mouse_sensitivity",
            Property,
            &[ParameterSpec::required("value", "FiniteF32")],
            TypeRef::domain("PropertyValue<FiniteF32>"),
            Some("rad/pixel"),
            LocalPresentation,
        ),
        "client.input.bindings" => binding.declaration(
            "client.input.bindings",
            Property,
            &[ParameterSpec::required("value", "InputBindings")],
            TypeRef::domain("PropertyValue<InputBindings>"),
            None,
            LocalPresentation,
        ),
        "client.graphics.sky_quality" | "sky quality" => binding.declaration(
            "client.graphics.sky_quality",
            Property,
            &[ParameterSpec::required("value", "SkyQuality")],
            TypeRef::domain("PropertyValue<SkyQuality>"),
            None,
            LocalPresentation,
        ),
        "client.debug.atmosphere" | "atmosphere debug" => binding.declaration(
            "client.debug.atmosphere",
            Property,
            &[ParameterSpec::required("value", "AtmosphereDebug")],
            TypeRef::domain("PropertyValue<AtmosphereDebug>"),
            None,
            LocalPresentation,
        ),
        "client.debug.moon_flux" => binding.declaration(
            "client.debug.moon_flux",
            Property,
            &[ParameterSpec::required("value", "Bool")],
            TypeRef::domain("PropertyValue<Bool>"),
            None,
            LocalPresentation,
        ),
        "client.debug.exposure_ev" => binding.declaration(
            "client.debug.exposure_ev",
            Property,
            &[ParameterSpec::required("value", "AutoOr<FiniteF32>")],
            TypeRef::domain("PropertyValue<AutoOr<FiniteF32>>"),
            Some("EV"),
            LocalPresentation,
        ),
        "client.debug.terrain" | "terrain debug" => binding.declaration(
            "client.debug.terrain",
            Property,
            &[ParameterSpec::required("value", "TerrainDebug")],
            TypeRef::domain("PropertyValue<TerrainDebug>"),
            None,
            LocalPresentation,
        ),
        "client.debug.clouds" | "weather clouds debug" => binding.declaration(
            "client.debug.clouds",
            Property,
            &[ParameterSpec::required("value", "CloudDebug")],
            TypeRef::domain("PropertyValue<CloudDebug>"),
            None,
            LocalPresentation,
        ),
        "config save" => binding.declaration(
            "client.config.save",
            Method,
            &[ParameterSpec::required("path", "Path")],
            TypeRef::domain("ConfigSaveReceipt"),
            None,
            FilesystemWrite,
        ),
        "dev look" => {
            binding.capability = Some("Renderer");
            binding.declaration(
                "client.camera.look",
                Method,
                &[
                    ParameterSpec::required("yaw", "FiniteF32"),
                    ParameterSpec::required("pitch", "FiniteF32"),
                ],
                TypeRef::domain("Unit"),
                Some("degrees"),
                LocalPresentation,
            )
        }
        _ => return None,
    })
}
