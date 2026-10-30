use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};
use crate::environment::quality::SkyQuality;
use crate::environment::weather::{WeatherCommand, WeatherPreset};
use minimium_platform::commands::parse_scale_ppm;

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "sky quality",
        alias: None,
        usage: "sky quality low|standard",
        parser: parse,
    },
    CommandSpec {
        name: "sky info",
        alias: None,
        usage: "sky info",
        parser: parse,
    },
    CommandSpec {
        name: "sky bodies",
        alias: None,
        usage: "sky bodies",
        parser: parse,
    },
    CommandSpec {
        name: "sky demo",
        alias: None,
        usage: "sky demo on|off (local rendering demo)",
        parser: parse,
    },
    CommandSpec {
        name: "fov",
        alias: None,
        usage: "fov [30..110] (vertical field of view in degrees; default 70)",
        parser: parse,
    },
    CommandSpec {
        name: "show_celestial_names",
        alias: None,
        usage: "show_celestial_names true|false",
        parser: parse,
    },
    CommandSpec {
        name: "weather status",
        alias: None,
        usage: "weather status",
        parser: parse,
    },
    CommandSpec {
        name: "weather live",
        alias: None,
        usage: "weather live on|off (MET Norway; sends rounded Earth coordinates)",
        parser: parse,
    },
    CommandSpec {
        name: "weather preset",
        alias: None,
        usage: "weather preset clear|partly|overcast|rain (local)",
        parser: parse,
    },
    CommandSpec {
        name: "time status",
        alias: None,
        usage: "time status",
        parser: parse,
    },
    CommandSpec {
        name: "time scale",
        alias: Some("timescale"),
        usage: "time scale DECIMAL (alias timescale; owned local server; 0.1-1000000x or 0)",
        parser: parse,
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    let [command, raw_arguments] = words else {
        return Err(ParseError::UnknownCommand);
    };
    let arguments = raw_arguments.split_whitespace().collect::<Vec<_>>();
    match (*command, arguments.as_slice()) {
        ("sky quality", [value]) => Ok(ClientCommand::SkyQuality(
            SkyQuality::parse(value).map_err(|_| ParseError::SkyQualityUsage)?,
        )),
        ("sky quality", _) => Err(ParseError::SkyQualityUsage),
        ("sky info", []) => Ok(ClientCommand::SkyInfo),
        ("sky bodies", []) => Ok(ClientCommand::SkyBodies),
        ("sky demo", ["on"]) => Ok(ClientCommand::SkyDemo(true)),
        ("sky demo", ["off"]) => Ok(ClientCommand::SkyDemo(false)),
        ("fov", []) => Ok(ClientCommand::Fov(None)),
        ("fov", [value]) => {
            let degrees = value.parse::<f32>().map_err(|_| ParseError::FovUsage)?;
            if !degrees.is_finite() || !(30.0..=110.0).contains(&degrees) {
                return Err(ParseError::FovUsage);
            }
            Ok(ClientCommand::Fov(Some(degrees)))
        }
        ("fov", _) => Err(ParseError::FovUsage),
        ("show_celestial_names", ["true"]) => Ok(ClientCommand::ShowCelestialNames(true)),
        ("show_celestial_names", ["false"]) => Ok(ClientCommand::ShowCelestialNames(false)),
        ("show_celestial_names", _) => Err(ParseError::CelestialNamesUsage),
        ("weather status", []) => Ok(ClientCommand::Weather(WeatherCommand::Status)),
        ("weather live", ["on"]) => Ok(ClientCommand::Weather(WeatherCommand::Live(true))),
        ("weather live", ["off"]) => Ok(ClientCommand::Weather(WeatherCommand::Live(false))),
        ("weather preset", ["clear"]) => Ok(ClientCommand::Weather(WeatherCommand::Preset(
            WeatherPreset::Clear,
        ))),
        ("weather preset", ["partly"]) => Ok(ClientCommand::Weather(WeatherCommand::Preset(
            WeatherPreset::PartlyCloudy,
        ))),
        ("weather preset", ["overcast"]) => Ok(ClientCommand::Weather(WeatherCommand::Preset(
            WeatherPreset::Overcast,
        ))),
        ("weather preset", ["rain"]) => Ok(ClientCommand::Weather(WeatherCommand::Preset(
            WeatherPreset::Rain,
        ))),
        ("weather status" | "weather live" | "weather preset", _) => Err(ParseError::WeatherUsage),
        ("time status", []) => Ok(ClientCommand::TimeStatus),
        ("time scale", [value]) => {
            parse_scale_ppm(value).map_err(|_| ParseError::TimeScaleUsage)?;
            Ok(ClientCommand::TimeScale((*value).to_owned()))
        }
        ("time scale", _) => Err(ParseError::TimeScaleUsage),
        _ => Err(ParseError::UnknownCommand),
    }
}
