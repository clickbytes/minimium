use super::ClientPropertyValue as Value;
use minimium_environment::time::{ClockMode, OfflinePolicy};
use minimium_renderer::{
    atmosphere_debug::AtmosphereDebug, cloud_presentation::CloudDebug, terrain_debug::TerrainDebug,
};

impl Value {
    /// Stable configuration/command encoding, independent of Rust Debug names.
    pub(crate) fn encode(&self) -> String {
        match self {
            Self::Locale(value) => match value {
                minimium_localization::Locale::English => "en",
                minimium_localization::Locale::Polish => "pl",
            }
            .into(),
            Self::CameraFov(value)
            | Self::MouseSensitivity(value)
            | Self::MeteorVolume(value)
            | Self::MovementSpeed(value)
            | Self::MovementAccel(value) => value.to_string(),
            Self::InputBindings(value) | Self::WorldTimeScale(value) => value.clone(),
            Self::SkyQuality(value) | Self::MeteorQuality(value) => value.label().into(),
            Self::SkyNames(value)
            | Self::MeteorAudio(value)
            | Self::MeteorEnabled(value)
            | Self::SkyDemo(value)
            | Self::MovementFly(value) => value.to_string(),
            Self::CloudDetail(value) => value.to_string(),
            Self::MeteorRate(value) => value.to_string(),
            Self::MeteorSeed(value) => value.map_or_else(|| "none".into(), |v| v.to_string()),
            Self::AtmosphereDebug(value) => match value {
                AtmosphereDebug::Off => "off",
                AtmosphereDebug::Transmission => "transmission",
                AtmosphereDebug::Scattering => "scattering",
                AtmosphereDebug::OpticalDepth => "optical_depth",
                AtmosphereDebug::Distance => "distance",
                AtmosphereDebug::Fog => "fog",
            }
            .into(),
            Self::TerrainDebug(value) => match value {
                TerrainDebug::Off => "off",
                TerrainDebug::Albedo => "albedo",
                TerrainDebug::Normals => "normals",
                TerrainDebug::Depth => "depth",
                TerrainDebug::Shadow => "shadow",
                TerrainDebug::BaseColor => "base_color",
                TerrainDebug::Coverage => "coverage",
                TerrainDebug::Habitat => "habitat",
                TerrainDebug::Source => "source",
            }
            .into(),
            Self::CloudDebug(value) => match value {
                CloudDebug::Off => "off",
                CloudDebug::Regions => "regions",
                CloudDebug::Layers => "layers",
                CloudDebug::Wind => "wind",
                CloudDebug::Kinds => "kinds",
                CloudDebug::Detail => "detail",
            }
            .into(),
            Self::WorldTimeMode(value) => match value {
                ClockMode::RealTime => "real_time",
                ClockMode::TimeScale => "time_scale",
            }
            .into(),
            Self::WorldTimeOfflinePolicy(value) => match value {
                OfflinePolicy::Pause => "pause",
                OfflinePolicy::Advance => "advance",
            }
            .into(),
        }
    }
}
