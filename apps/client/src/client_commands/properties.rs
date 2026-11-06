use super::{command_specs, ClientCommand, ParseError};
use catalog::registry;
use minimium_commands::{parse_property_request, CommandError, CommandErrorCode, PropertyRequest};
use minimium_environment::time::{ClockMode, OfflinePolicy};
use minimium_environment::weather::CloudDebug;
use minimium_renderer::{
    atmosphere_debug::AtmosphereDebug, quality::SkyQuality, terrain_debug::TerrainDebug,
};

mod catalog;

pub(crate) fn property_specs() -> impl Iterator<Item = &'static minimium_commands::PropertySpec> {
    catalog::registry().specs()
}

pub(crate) fn property_spec(key: &str) -> Option<&'static minimium_commands::PropertySpec> {
    catalog::registry().get(key)
}

pub(crate) fn format_property_help(spec: &minimium_commands::PropertySpec) -> String {
    catalog::format_property_help(spec)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClientPropertyKey {
    CameraFov,
    MouseSensitivity,
    SkyQuality,
    SkyNames,
    CloudDetail,
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClientPropertyValue {
    CameraFov(f32),
    MouseSensitivity(f32),
    SkyQuality(SkyQuality),
    SkyNames(bool),
    CloudDetail(u8),
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ClientPropertyCommand {
    Get(ClientPropertyKey),
    Set(ClientPropertyValue),
    Reset(ClientPropertyKey),
    Print(String),
}

pub(crate) fn format_time_value(
    key: ClientPropertyKey,
    frame: crate::environment::EnvironmentFrameInput,
) -> Option<String> {
    use minimium_commands::{PropertySource, PropertyValue};
    let requested = match key {
        ClientPropertyKey::WorldTimeMode => frame
            .clock_diagnostics
            .and_then(|clock| clock.mode)
            .map(|mode| match mode {
                ClockMode::RealTime => "real_time".to_owned(),
                ClockMode::TimeScale => "time_scale".to_owned(),
            }),
        ClientPropertyKey::WorldTimeScale => frame
            .rate_ppm
            .map(|ppm| format!("{:.6}", ppm as f64 / 1_000_000.0)),
        ClientPropertyKey::WorldTimeOfflinePolicy => frame
            .clock_diagnostics
            .and_then(|clock| clock.offline_policy)
            .map(|policy| match policy {
                OfflinePolicy::Pause => "pause".to_owned(),
                OfflinePolicy::Advance => "advance".to_owned(),
            }),
        ClientPropertyKey::WorldTimeUtc => frame
            .render_world_utc_ms
            .map(minimium_client_runtime::environment::format_utc),
        ClientPropertyKey::WorldTimeRevision => frame.revision.map(|revision| revision.to_string()),
        _ => return None,
    };
    let source = if requested.is_none() {
        PropertySource::Unavailable
    } else if frame.clock_diagnostics.is_some_and(|clock| {
        clock.source == minimium_client_runtime::environment::ClockSource::Demo
    }) {
        PropertySource::Runtime
    } else {
        PropertySource::Replica
    };
    let metadata = PropertyValue {
        requested: requested.clone(),
        effective: requested,
        default: None,
        source,
        difference_reason: Some(if source == PropertySource::Unavailable {
            "clock replica unavailable".to_owned()
        } else {
            "client replica; administrative state may differ until synchronized".to_owned()
        }),
    };
    Some(format!(
        "{} requested={} effective={} default=n/a source={} reason={}",
        key.key(),
        metadata.requested.as_deref().unwrap_or("unavailable"),
        metadata.effective.as_deref().unwrap_or("unavailable"),
        metadata.source.as_str(),
        metadata.difference_reason.as_deref().unwrap_or("none"),
    ))
}

impl ClientPropertyKey {
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::CameraFov => "client.camera.fov",
            Self::MouseSensitivity => "client.input.mouse_sensitivity",
            Self::SkyQuality => "client.graphics.sky_quality",
            Self::SkyNames => "client.sky.names",
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

    fn parse(key: &str) -> Option<Self> {
        registry().get(key)?;
        Some(match key {
            "client.camera.fov" => Self::CameraFov,
            "client.input.mouse_sensitivity" => Self::MouseSensitivity,
            "client.graphics.sky_quality" => Self::SkyQuality,
            "client.sky.names" => Self::SkyNames,
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

pub(crate) fn parse_setting_value(
    key: &str,
    value: &str,
) -> Result<ClientPropertyValue, CommandError> {
    parse_value(require_key(key)?, value)
}

pub(super) fn parse(input: &str) -> Result<Option<ClientCommand>, ParseError> {
    let request = parse_property_request(input).map_err(ParseError::Structured)?;
    let Some(request) = request else {
        return Ok(None);
    };
    if matches!(request, PropertyRequest::Help { target: None }) {
        return Ok(None);
    }
    parse_request(request)
        .map(|command| command.map(ClientCommand::Property))
        .map_err(ParseError::Structured)
}

fn parse_request(request: PropertyRequest) -> Result<Option<ClientPropertyCommand>, CommandError> {
    match request {
        PropertyRequest::Get { key } => Ok(Some(ClientPropertyCommand::Get(require_key(&key)?))),
        PropertyRequest::Set { key, value } => {
            let key = require_key(&key)?;
            Ok(Some(ClientPropertyCommand::Set(parse_value(key, &value)?)))
        }
        PropertyRequest::Reset { key } => {
            let key = require_key(&key)?;
            let spec = registry().get(key.key()).expect("key has a descriptor");
            if !spec.resettable {
                return Err(CommandError::new(
                    CommandErrorCode::InvalidArguments,
                    Some(key.key()),
                    "property cannot be reset",
                    Some("reset KEY"),
                ));
            }
            Ok(Some(ClientPropertyCommand::Reset(key)))
        }
        PropertyRequest::List { prefix } => {
            let items = registry().list(prefix.as_deref()).collect::<Vec<_>>();
            if items.is_empty() {
                return Ok(Some(ClientPropertyCommand::Print(format!(
                    "no client properties match {}",
                    prefix.as_deref().unwrap_or("the requested prefix")
                ))));
            }
            Ok(Some(ClientPropertyCommand::Print(
                items
                    .into_iter()
                    .map(|spec| format!("{} — {}", spec.key, spec.description))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )))
        }
        PropertyRequest::Help { target: None } => Ok(None),
        PropertyRequest::Help {
            target: Some(target),
        } => {
            if let Some(spec) = registry().get(&target) {
                return Ok(Some(ClientPropertyCommand::Print(format_property_help(
                    spec,
                ))));
            }
            if let Some(spec) = command_specs()
                .iter()
                .find(|spec| spec.name == target || spec.alias == Some(target.as_str()))
            {
                return Ok(Some(ClientPropertyCommand::Print(spec.usage.to_owned())));
            }
            let grouped = command_specs()
                .iter()
                .filter(|spec| {
                    spec.name.starts_with(&format!("{target} "))
                        || spec
                            .alias
                            .is_some_and(|alias| alias.starts_with(&format!("{target} ")))
                })
                .map(|spec| spec.usage)
                .collect::<Vec<_>>();
            if !grouped.is_empty() {
                return Ok(Some(ClientPropertyCommand::Print(grouped.join("\n"))));
            }
            Err(CommandError::new(
                CommandErrorCode::HelpQuery,
                Some(target),
                "no command or property has that name",
                Some("help [COMMAND|KEY]"),
            ))
        }
    }
}

fn require_key(key: &str) -> Result<ClientPropertyKey, CommandError> {
    ClientPropertyKey::parse(key).ok_or_else(|| {
        CommandError::new(
            CommandErrorCode::UnknownProperty,
            Some(key),
            "unknown client property",
            Some("list [PREFIX]"),
        )
    })
}

fn parse_value(key: ClientPropertyKey, value: &str) -> Result<ClientPropertyValue, CommandError> {
    let invalid = |reason: String| {
        CommandError::new(
            CommandErrorCode::InvalidValue,
            Some(key.key()),
            reason,
            Some("set KEY VALUE"),
        )
    };
    match key {
        ClientPropertyKey::CameraFov => super::parse_fov_degrees(value)
            .map(ClientPropertyValue::CameraFov)
            .map_err(|error| invalid(error.to_owned())),
        ClientPropertyKey::MouseSensitivity => {
            let number = value
                .parse::<f32>()
                .map_err(|_| invalid("sensitivity must be a finite number in 0..=0.1".into()))?;
            if !number.is_finite() || !(0.0..=0.1).contains(&number) {
                return Err(invalid(
                    "sensitivity must be a finite number in 0..=0.1".into(),
                ));
            }
            Ok(ClientPropertyValue::MouseSensitivity(number))
        }
        ClientPropertyKey::SkyQuality => SkyQuality::parse(value)
            .map(ClientPropertyValue::SkyQuality)
            .map_err(|_| invalid("quality must be low or standard".into())),
        ClientPropertyKey::SkyNames | ClientPropertyKey::SkyDemo => {
            let enabled =
                parse_bool(value).ok_or_else(|| invalid("expected true or false".into()))?;
            if key == ClientPropertyKey::SkyNames {
                Ok(ClientPropertyValue::SkyNames(enabled))
            } else {
                Ok(ClientPropertyValue::SkyDemo(enabled))
            }
        }
        ClientPropertyKey::CloudDetail => {
            let level = value
                .parse::<u8>()
                .map_err(|_| invalid("cloud detail must be an integer from 2 through 6".into()))?;
            if !(2..=6).contains(&level) {
                return Err(invalid(
                    "cloud detail must be an integer from 2 through 6".into(),
                ));
            }
            Ok(ClientPropertyValue::CloudDetail(level))
        }
        ClientPropertyKey::AtmosphereDebug => parse_atmosphere_debug(value)
            .map(ClientPropertyValue::AtmosphereDebug)
            .ok_or_else(|| invalid("unknown atmosphere debug mode".into())),
        ClientPropertyKey::TerrainDebug => parse_terrain_debug(value)
            .map(ClientPropertyValue::TerrainDebug)
            .ok_or_else(|| invalid("unknown terrain debug mode".into())),
        ClientPropertyKey::CloudDebug => parse_cloud_debug(value)
            .map(ClientPropertyValue::CloudDebug)
            .ok_or_else(|| invalid("unknown cloud debug mode".into())),
        ClientPropertyKey::MovementFly => parse_bool(value)
            .map(ClientPropertyValue::MovementFly)
            .ok_or_else(|| invalid("expected true or false".into())),
        ClientPropertyKey::MovementSpeed | ClientPropertyKey::MovementAccel => {
            let number = value
                .parse::<f32>()
                .map_err(|_| invalid("movement value must be finite and in 0..=10000".into()))?;
            if !number.is_finite() || !(0.0..=minimium_simulation::MOVEMENT_LIMIT).contains(&number)
            {
                return Err(invalid(
                    "movement value must be finite and in 0..=10000".into(),
                ));
            }
            if key == ClientPropertyKey::MovementSpeed {
                Ok(ClientPropertyValue::MovementSpeed(number))
            } else {
                Ok(ClientPropertyValue::MovementAccel(number))
            }
        }
        ClientPropertyKey::WorldTimeMode => match value {
            "real_time" => Ok(ClientPropertyValue::WorldTimeMode(ClockMode::RealTime)),
            "time_scale" => Ok(ClientPropertyValue::WorldTimeMode(ClockMode::TimeScale)),
            _ => Err(invalid("mode must be real_time or time_scale".into())),
        },
        ClientPropertyKey::WorldTimeScale => minimium_environment::parse_scale_ppm(value)
            .map(|_| ClientPropertyValue::WorldTimeScale(value.to_owned()))
            .map_err(invalid),
        ClientPropertyKey::WorldTimeOfflinePolicy => match value {
            "pause" => Ok(ClientPropertyValue::WorldTimeOfflinePolicy(
                OfflinePolicy::Pause,
            )),
            "advance" => Ok(ClientPropertyValue::WorldTimeOfflinePolicy(
                OfflinePolicy::Advance,
            )),
            _ => Err(invalid("offline policy must be pause or advance".into())),
        },
        ClientPropertyKey::WorldTimeUtc | ClientPropertyKey::WorldTimeRevision => {
            Err(invalid("property is read-only".into()))
        }
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_atmosphere_debug(value: &str) -> Option<AtmosphereDebug> {
    Some(match value {
        "off" => AtmosphereDebug::Off,
        "transmission" => AtmosphereDebug::Transmission,
        "scattering" => AtmosphereDebug::Scattering,
        "optical_depth" => AtmosphereDebug::OpticalDepth,
        "distance" => AtmosphereDebug::Distance,
        "fog" => AtmosphereDebug::Fog,
        _ => return None,
    })
}

fn parse_terrain_debug(value: &str) -> Option<TerrainDebug> {
    Some(match value {
        "off" => TerrainDebug::Off,
        "albedo" => TerrainDebug::Albedo,
        "normals" => TerrainDebug::Normals,
        "depth" => TerrainDebug::Depth,
        "shadow" => TerrainDebug::Shadow,
        "base_color" => TerrainDebug::BaseColor,
        "coverage" => TerrainDebug::Coverage,
        "habitat" => TerrainDebug::Habitat,
        "source" => TerrainDebug::Source,
        _ => return None,
    })
}

fn parse_cloud_debug(value: &str) -> Option<CloudDebug> {
    Some(match value {
        "off" => CloudDebug::Off,
        "regions" => CloudDebug::Regions,
        "layers" => CloudDebug::Layers,
        "wind" => CloudDebug::Wind,
        "kinds" => CloudDebug::Kinds,
        "detail" => CloudDebug::Detail,
        _ => return None,
    })
}
