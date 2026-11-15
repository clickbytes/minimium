use minimium_environment::time::{ClockMode, OfflinePolicy};
use minimium_environment::weather::CloudDebug;
use minimium_renderer::{
    atmosphere_debug::AtmosphereDebug, quality::SkyQuality, terrain_debug::TerrainDebug,
};

macro_rules! property_keys {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) enum ClientPropertyKey { $($variant),+ }
        impl ClientPropertyKey {
            pub(crate) const fn key(self) -> &'static str {
                match self { $(Self::$variant => $name),+ }
            }
            pub(crate) fn parse(key: &str) -> Option<Self> {
                match key {
                    $($name => Some(Self::$variant),)+
                    "client.audio.meteors" => Some(Self::MeteorAudio),
                    _ => None,
                }
            }
        }
    };
}

property_keys! {
    Locale => "ui.locale",
    CameraFov => "client.camera.fov",
    MouseSensitivity => "client.input.mouse_sensitivity",
    InputBindings => "client.input.bindings",
    SkyQuality => "client.graphics.sky_quality",
    SkyNames => "client.sky.names",
    MeteorAudio => "client.sky.meteors.sound",
    MeteorEnabled => "client.sky.meteors.enabled",
    MeteorRate => "client.sky.meteors.rate",
    MeteorQuality => "client.sky.meteors.quality",
    MeteorSeed => "client.sky.meteors.seed_override",
    MeteorVolume => "client.audio.meteor_volume",
    CloudDetail => "client.weather.cloud_detail",
    AtmosphereDebug => "client.debug.atmosphere",
    TerrainDebug => "client.debug.terrain",
    CloudDebug => "client.debug.clouds",
    SkyDemo => "client.sky.demo",
    MovementFly => "player.movement.fly",
    MovementSpeed => "player.movement.speed",
    MovementAccel => "player.movement.accel",
    WorldTimeMode => "world.time.mode",
    WorldTimeScale => "world.time.scale",
    WorldTimeOfflinePolicy => "world.time.offline_policy",
    WorldTimeUtc => "world.time.utc",
    WorldTimeRevision => "world.time.revision",
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
