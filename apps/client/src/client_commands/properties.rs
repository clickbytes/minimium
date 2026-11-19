use super::{catalog::catalog, ClientCommand, ParseError};
use catalog::registry;
use minimium_commands::{parse_property_request, CommandError, CommandErrorCode, PropertyRequest};
use minimium_environment::time::{ClockMode, OfflinePolicy};
use minimium_renderer::cloud_presentation::CloudDebug;
use minimium_renderer::{
    atmosphere_debug::AtmosphereDebug, quality::SkyQuality, terrain_debug::TerrainDebug,
};

mod catalog;
mod encoding;
mod types;
pub(crate) use types::{ClientPropertyCommand, ClientPropertyKey, ClientPropertyValue};

pub(crate) fn property_specs() -> impl Iterator<Item = &'static minimium_commands::PropertySpec> {
    catalog::registry().specs()
}

/// Canonical, local startup settings. Aliases and authoritative world properties
/// remain available to commands but do not create duplicate startup records.
pub(crate) fn local_setting_specs() -> impl Iterator<Item = &'static minimium_commands::PropertySpec>
{
    property_specs().filter(|spec| {
        spec.permission == minimium_commands::PropertyPermission::Local
            && spec.lifecycle == minimium_commands::PropertyLifecycle::Live
            && spec.default.is_some()
            && ClientPropertyKey::parse(spec.key).is_some_and(|key| key.key() == spec.key)
    })
}

pub(crate) fn property_spec(key: &str) -> Option<&'static minimium_commands::PropertySpec> {
    catalog::registry().get(ClientPropertyKey::parse(key)?.key())
}

pub(crate) fn format_property_help_localized(
    spec: &minimium_commands::PropertySpec,
    localizer: &minimium_localization::Localizer,
    locale: minimium_localization::Locale,
) -> String {
    let description = localizer.format(
        locale,
        &minimium_localization::UiMessage::PropertyDescription(spec.key.to_owned()),
    );
    let details = catalog()
        .resolve(spec.key)
        .map(|entry| entry.describe())
        .unwrap_or_default();
    format!(
        "{}\n{details}",
        catalog::format_property_help_with_description(spec, &description)
    )
}

pub(crate) fn format_time_value(
    key: ClientPropertyKey,
    frame: crate::environment::EnvironmentFrameInput,
) -> Option<String> {
    super::time_adapter::property(key, frame).map(|p| {
        format!(
            "{} {}",
            key.key(),
            p.format_with(minimium_commands::operation::CommandValue::encode)
        )
    })
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

pub(super) fn parse_request(
    request: PropertyRequest,
) -> Result<Option<ClientPropertyCommand>, CommandError> {
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
        PropertyRequest::List { prefix } => Ok(Some(ClientPropertyCommand::List(prefix))),
        PropertyRequest::Help { target: None } => Ok(None),
        PropertyRequest::Help {
            target: Some(target),
        } => {
            if let Some(entry) = catalog().resolve(&target) {
                if let Some(property) = entry.property {
                    return Ok(Some(ClientPropertyCommand::Help(require_key(
                        property.key,
                    )?)));
                }
                return Ok(Some(ClientPropertyCommand::CommandHelp(target)));
            }
            if !super::help::group_entries(&target).is_empty() {
                return Ok(Some(ClientPropertyCommand::CommandHelpGroup(target)));
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
        ClientPropertyKey::Locale => minimium_localization::Locale::from_setting(value)
            .map(ClientPropertyValue::Locale)
            .ok_or_else(|| invalid("locale must be en or pl".into())),
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
        ClientPropertyKey::InputBindings => {
            crate::input::InputMapper::from_setting(value).map_err(invalid)?;
            Ok(ClientPropertyValue::InputBindings(value.to_owned()))
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
        ClientPropertyKey::MeteorEnabled => parse_bool(value)
            .map(ClientPropertyValue::MeteorEnabled)
            .ok_or_else(|| invalid("expected true or false".into())),
        ClientPropertyKey::MeteorQuality => SkyQuality::parse(value)
            .map(ClientPropertyValue::MeteorQuality)
            .map_err(|_| invalid("quality must be low or standard".into())),
        ClientPropertyKey::MeteorSeed => {
            let seed = if value == "none" {
                None
            } else {
                Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| invalid("expected none or decimal u64".into()))?,
                )
            };
            Ok(ClientPropertyValue::MeteorSeed(seed))
        }
        ClientPropertyKey::MeteorRate => {
            let rate = value
                .parse::<f64>()
                .map_err(|_| invalid("expected finite 0..=10".into()))?;
            if !rate.is_finite() || !(0.0..=10.0).contains(&rate) {
                return Err(invalid("expected finite 0..=10".into()));
            }
            Ok(ClientPropertyValue::MeteorRate(rate))
        }
        ClientPropertyKey::MeteorAudio => parse_bool(value)
            .map(ClientPropertyValue::MeteorAudio)
            .ok_or_else(|| invalid("expected true or false".into())),
        ClientPropertyKey::MeteorVolume => {
            let volume = value
                .parse::<f32>()
                .map_err(|_| invalid("volume must be finite 0..=1".into()))?;
            if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
                return Err(invalid("volume must be finite 0..=1".into()));
            }
            Ok(ClientPropertyValue::MeteorVolume(volume))
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
        ClientPropertyKey::MoonFlux => parse_bool(value)
            .map(ClientPropertyValue::MoonFlux)
            .ok_or_else(|| invalid("expected true or false".into())),
        ClientPropertyKey::ExposureEv => {
            if value == "auto" {
                return Ok(ClientPropertyValue::ExposureEv(None));
            }
            value
                .parse::<f32>()
                .ok()
                .filter(|ev| (-8.0..=24.0).contains(ev))
                .map(|ev| ClientPropertyValue::ExposureEv(Some(ev)))
                .ok_or_else(|| invalid("expected auto or exposure stops from -8 to 24".into()))
        }
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
            Err(CommandError::new(
                CommandErrorCode::ReadOnly,
                Some(key.key()),
                "property is read-only",
                Some("get KEY"),
            ))
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

pub(super) fn parse_atmosphere_debug(value: &str) -> Option<AtmosphereDebug> {
    Some(match value {
        "off" => AtmosphereDebug::Off,
        "transmission" => AtmosphereDebug::Transmission,
        "scattering" => AtmosphereDebug::Scattering,
        "optical_depth" => AtmosphereDebug::OpticalDepth,
        "distance" => AtmosphereDebug::Distance,
        "fog" => AtmosphereDebug::Fog,
        "fog_shadow" => AtmosphereDebug::FogShadow,
        _ => return None,
    })
}

pub(super) fn parse_terrain_debug(value: &str) -> Option<TerrainDebug> {
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
        "moon_direct" => TerrainDebug::MoonDirect,
        "moon_diffuse" => TerrainDebug::MoonDiffuse,
        "background" => TerrainDebug::Background,
        "moon_coverage" => TerrainDebug::MoonCoverage,
        "light_channels" => TerrainDebug::LightChannels,
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
