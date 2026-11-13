use minimium_environment::time::{ClockMode, OfflinePolicy};
use minimium_environment::weather::CloudDebug;
use minimium_renderer::{
    atmosphere_debug::AtmosphereDebug, quality::SkyQuality, terrain_debug::TerrainDebug,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClientPropertyKey {
    Locale,
    CameraFov,
    MouseSensitivity,
    InputBindings,
    SkyQuality,
    SkyNames,
    CloudDetail,
    MeteorAudio,
    MeteorVolume,
    MeteorEnabled,
    MeteorRate,
    MeteorQuality,
    MeteorSeed,

    AtmosphereDebug,
    TerrainDebug,
    CloudDebug,
    SkyDemo,
    MovementFly,
    MovementSpeed,
    MovementAccel,
    WorldTimeMode,
    WorldTimeScale,
    WorldTimeOfflinePolicy,
    WorldTimeUtc,
    WorldTimeRevision,
}

impl ClientPropertyKey {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::Locale => "ui.locale",
            Self::CameraFov => "client.camera.fov",
            Self::MouseSensitivity => "client.input.mouse_sensitivity",
            Self::InputBindings => "client.input.bindings",
            Self::SkyQuality => "client.graphics.sky_quality",
            Self::SkyNames => "client.sky.names",
            Self::MeteorAudio => "client.sky.meteors.sound",
            Self::MeteorEnabled => "client.sky.meteors.enabled",
            Self::MeteorRate => "client.sky.meteors.rate",
            Self::MeteorQuality => "client.sky.meteors.quality",
            Self::MeteorSeed => "client.sky.meteors.seed_override",
            Self::MeteorVolume => "client.audio.meteor_volume",
            Self::CloudDetail => "client.weather.cloud_detail",
            Self::AtmosphereDebug => "client.debug.atmosphere",
            Self::TerrainDebug => "client.debug.terrain",
            Self::CloudDebug => "client.debug.clouds",
            Self::SkyDemo => "client.sky.demo",
            Self::MovementFly => "player.movement.fly",
            Self::MovementSpeed => "player.movement.speed",
            Self::MovementAccel => "player.movement.accel",
            Self::WorldTimeMode => "world.time.mode",
            Self::WorldTimeScale => "world.time.scale",
            Self::WorldTimeOfflinePolicy => "world.time.offline_policy",
            Self::WorldTimeUtc => "world.time.utc",
            Self::WorldTimeRevision => "world.time.revision",
        }
    }

    pub(super) fn parse(key: &str) -> Option<Self> {
        super::registry().get(key)?;
        Some(match key {
            "ui.locale" => Self::Locale,
            "client.camera.fov" => Self::CameraFov,
            "client.input.mouse_sensitivity" => Self::MouseSensitivity,
            "client.input.bindings" => Self::InputBindings,
            "client.graphics.sky_quality" => Self::SkyQuality,
            "client.sky.names" => Self::SkyNames,
            "client.sky.meteors.sound" => Self::MeteorAudio,
            "client.sky.meteors.enabled" => Self::MeteorEnabled,
            "client.sky.meteors.rate" => Self::MeteorRate,
            "client.sky.meteors.quality" => Self::MeteorQuality,
            "client.sky.meteors.seed_override" => Self::MeteorSeed,
            "client.audio.meteors" => Self::MeteorAudio,
            "client.audio.meteor_volume" => Self::MeteorVolume,
            "client.weather.cloud_detail" => Self::CloudDetail,
            "client.debug.atmosphere" => Self::AtmosphereDebug,
            "client.debug.terrain" => Self::TerrainDebug,
            "client.debug.clouds" => Self::CloudDebug,
            "client.sky.demo" => Self::SkyDemo,
            "player.movement.fly" => Self::MovementFly,
            "player.movement.speed" => Self::MovementSpeed,
            "player.movement.accel" => Self::MovementAccel,
            "world.time.mode" => Self::WorldTimeMode,
            "world.time.scale" => Self::WorldTimeScale,
            "world.time.offline_policy" => Self::WorldTimeOfflinePolicy,
            "world.time.utc" => Self::WorldTimeUtc,
            "world.time.revision" => Self::WorldTimeRevision,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClientPropertyValue {
    Locale(minimium_localization::Locale),
    CameraFov(f32),
    MouseSensitivity(f32),
    InputBindings(String),
    SkyQuality(SkyQuality),
    SkyNames(bool),
    CloudDetail(u8),
    MeteorAudio(bool),
    MeteorVolume(f32),
    MeteorEnabled(bool),
    MeteorRate(f64),
    MeteorQuality(SkyQuality),
    MeteorSeed(Option<u64>),

    AtmosphereDebug(AtmosphereDebug),
    TerrainDebug(TerrainDebug),
    CloudDebug(CloudDebug),
    SkyDemo(bool),
    MovementFly(bool),
    MovementSpeed(f32),
    MovementAccel(f32),
    WorldTimeMode(ClockMode),
    WorldTimeScale(String),
    WorldTimeOfflinePolicy(OfflinePolicy),
}

impl ClientPropertyValue {
    pub(crate) fn key(&self) -> ClientPropertyKey {
        match self {
            Self::Locale(_) => ClientPropertyKey::Locale,
            Self::CameraFov(_) => ClientPropertyKey::CameraFov,
            Self::MouseSensitivity(_) => ClientPropertyKey::MouseSensitivity,
            Self::InputBindings(_) => ClientPropertyKey::InputBindings,
            Self::SkyQuality(_) => ClientPropertyKey::SkyQuality,
            Self::SkyNames(_) => ClientPropertyKey::SkyNames,
            Self::MeteorAudio(_) => ClientPropertyKey::MeteorAudio,
            Self::MeteorEnabled(_) => ClientPropertyKey::MeteorEnabled,
            Self::MeteorRate(_) => ClientPropertyKey::MeteorRate,
            Self::MeteorQuality(_) => ClientPropertyKey::MeteorQuality,
            Self::MeteorSeed(_) => ClientPropertyKey::MeteorSeed,
            Self::MeteorVolume(_) => ClientPropertyKey::MeteorVolume,
            Self::CloudDetail(_) => ClientPropertyKey::CloudDetail,
            Self::AtmosphereDebug(_) => ClientPropertyKey::AtmosphereDebug,
            Self::TerrainDebug(_) => ClientPropertyKey::TerrainDebug,
            Self::CloudDebug(_) => ClientPropertyKey::CloudDebug,
            Self::SkyDemo(_) => ClientPropertyKey::SkyDemo,
            Self::MovementFly(_) => ClientPropertyKey::MovementFly,
            Self::MovementSpeed(_) => ClientPropertyKey::MovementSpeed,
            Self::MovementAccel(_) => ClientPropertyKey::MovementAccel,
            Self::WorldTimeMode(_) => ClientPropertyKey::WorldTimeMode,
            Self::WorldTimeScale(_) => ClientPropertyKey::WorldTimeScale,
            Self::WorldTimeOfflinePolicy(_) => ClientPropertyKey::WorldTimeOfflinePolicy,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClientPropertyCommand {
    Get(ClientPropertyKey),
    Set(ClientPropertyValue),
    Reset(ClientPropertyKey),
    Help(ClientPropertyKey),
    List(Option<String>),
    CommandHelp(String),
    CommandHelpGroup(String),
}
