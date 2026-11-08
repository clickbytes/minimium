use super::{
    ArgumentSpec as Arg, ArgumentType as ArgType, ClientCommand, CommandRegistry, CommandSpec,
    ParseError,
};
use minimium_environment::parse_scale_ppm;
use minimium_renderer::quality::SkyQuality;

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "set_time",
        alias: None,
        usage: "set_time HHMM (UTC; owned local server)",
        arguments: &[Arg::new(
            "time",
            ArgType::Text,
            false,
            false,
            None,
            Some("0000..2359"),
            Some("UTC HHMM"),
            "UTC wall time",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "set_time_local",
        alias: None,
        usage: "set_time_local HHMM (local mean solar time at player; owned local server)",
        arguments: &[Arg::new(
            "time",
            ArgType::Text,
            false,
            false,
            None,
            Some("0000..2359"),
            Some("HHMM"),
            "local mean solar time at player",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "sky quality",
        alias: None,
        usage: "sky quality low|standard",
        arguments: &[Arg::new(
            "quality",
            ArgType::Enumeration,
            false,
            false,
            Some(&["low", "standard"]),
            None,
            None,
            "requested sky rendering quality",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "sky info",
        alias: None,
        usage: "sky info",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "sky bodies",
        alias: None,
        usage: "sky bodies",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "sky demo",
        alias: None,
        usage: "sky demo on|off (local rendering demo)",
        arguments: &[Arg::new(
            "enabled",
            ArgType::Boolean,
            false,
            false,
            Some(&["on", "off"]),
            None,
            None,
            "toggle local sky demo",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "camera fov",
        alias: Some("fov"),
        usage: "camera fov [30..110] (vertical field of view in degrees; default 70)",
        arguments: &[Arg::new(
            "degrees",
            ArgType::Decimal,
            true,
            false,
            None,
            Some("30..=110"),
            Some("degrees"),
            "vertical field of view",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "sky names",
        alias: Some("show_celestial_names"),
        usage: "sky names true|false",
        arguments: &[Arg::new(
            "enabled",
            ArgType::Boolean,
            false,
            false,
            Some(&["true", "false"]),
            None,
            None,
            "show celestial body names",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "time status",
        alias: None,
        usage: "time status",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "spawn_random_bolid",
        alias: None,
        usage: "spawn_random_bolid (owned local server)",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "time scale",
        alias: Some("timescale"),
        usage: "time scale DECIMAL (alias timescale; owned local server; 0.1-1000000x or 0)",
        arguments: &[Arg::new(
            "scale",
            ArgType::Decimal,
            false,
            false,
            None,
            Some("0 or 0.1..=1000000"),
            Some("x"),
            "world clock speed multiplier",
        )],
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
        ("camera fov", []) => Ok(ClientCommand::Fov(None)),
        ("camera fov", [value]) => Ok(ClientCommand::Fov(Some(parse_fov_value(value)?))),
        ("camera fov", _) => Err(ParseError::FovUsage),
        ("sky names", ["true"]) => Ok(ClientCommand::ShowCelestialNames(true)),
        ("sky names", ["false"]) => Ok(ClientCommand::ShowCelestialNames(false)),
        ("sky names", _) => Err(ParseError::CelestialNamesUsage),
        (name @ ("set_time" | "set_time_local"), [value]) => {
            let minute = minimium_environment::clock_command::parse_hhmm(value)
                .map_err(|_| ParseError::SetTimeUsage)?;
            Ok(ClientCommand::SetTime {
                minute,
                local: name == "set_time_local",
            })
        }
        ("set_time" | "set_time_local", _) => Err(ParseError::SetTimeUsage),
        ("time status", []) => Ok(ClientCommand::TimeStatus),
        ("spawn_random_bolid", []) => Ok(ClientCommand::SpawnRandomBolid),
        ("time scale", [value]) => {
            parse_scale_ppm(value).map_err(|_| ParseError::TimeScaleUsage)?;
            Ok(ClientCommand::TimeScale((*value).to_owned()))
        }
        ("time scale", _) => Err(ParseError::TimeScaleUsage),
        _ => Err(ParseError::UnknownCommand),
    }
}

pub(super) fn parse_fov_value(value: &str) -> Result<f32, ParseError> {
    super::parse_fov_degrees(value).map_err(|_| ParseError::FovUsage)
}
