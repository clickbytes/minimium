use super::{
    ArgumentSpec as Arg, ArgumentType as ArgType, ClientCommand, CommandRegistry, CommandSpec,
    ParseError,
};
use minimium_environment::parse_scale_ppm;
use minimium_renderer::quality::SkyQuality;

use super::MeteorPreset;

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "sky meteors debug",
        alias: None,
        usage: "sky meteors debug off|light|occlusion|paths|budget",
        arguments: &[Arg::new(
            "mode",
            ArgType::Enumeration,
            false,
            false,
            Some(&["off", "light", "occlusion", "paths", "budget"]),
            None,
            None,
            "local diagnostic",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "sky meteors seed",
        alias: None,
        usage: "sky meteors seed VALUE (decimal u64; session only)",
        arguments: &[Arg::new(
            "seed",
            ArgType::Integer,
            false,
            false,
            None,
            Some("u64"),
            None,
            "session seed override",
        )],
        parser: parse,
    },
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
        name: "sky meteors status",
        alias: None,
        usage: "sky meteors status",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "sky meteors spawn",
        alias: None,
        usage: "sky meteors spawn PRESET [seed]",
        arguments: &[
            Arg::new(
                "preset",
                ArgType::Enumeration,
                false,
                false,
                Some(super::meteors::PRESET_NAMES),
                None,
                None,
                "manual ambient meteor preset",
            ),
            Arg::new(
                "seed",
                ArgType::Integer,
                true,
                false,
                None,
                Some("u64"),
                None,
                "deterministic seed",
            ),
        ],
        parser: parse,
    },
    CommandSpec {
        name: "sky meteors debug light",
        alias: None,
        usage: "sky meteors debug light [on|off|transmission]",
        arguments: &[Arg::new(
            "enabled",
            ArgType::Enumeration,
            true,
            false,
            Some(&["on", "off", "transmission"]),
            None,
            None,
            "local meteor lighting diagnostic",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "sky meteors clear",
        alias: None,
        usage: "sky meteors clear",
        arguments: &[],
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
        usage: "spawn_random_bolid [N: 1..64] (irregular shower; owned local server)",
        arguments: &[Arg::new(
            "count",
            ArgType::Integer,
            true,
            false,
            None,
            Some("1..64"),
            Some("bolids"),
            "number of bolids, default 1",
        )],
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
        ("sky meteors seed", [seed]) => {
            let seed = seed.parse::<u64>().map_err(|_| ParseError::MeteorUsage)?;
            Ok(ClientCommand::Property(super::ClientPropertyCommand::Set(
                super::ClientPropertyValue::MeteorSeed(Some(seed)),
            )))
        }
        ("sky meteors seed", _) => Err(ParseError::Usage("sky meteors seed VALUE (decimal u64)")),
        ("sky meteors debug", [mode]) => Ok(ClientCommand::SkyMeteorsDebug(
            super::MeteorDebug::parse(mode).ok_or(ParseError::Usage(
                "sky meteors debug off|light|occlusion|paths|budget",
            ))?,
        )),
        ("sky meteors debug", _) => Err(ParseError::Usage(
            "sky meteors debug off|light|occlusion|paths|budget",
        )),
        ("sky meteors status", []) => Ok(ClientCommand::SkyMeteorsStatus),
        ("sky meteors spawn", [value]) => Ok(ClientCommand::SkyMeteorsSpawn(
            MeteorPreset::parse(value).ok_or(ParseError::MeteorUsage)?,
            None,
        )),
        ("sky meteors spawn", [value, seed]) => Ok(ClientCommand::SkyMeteorsSpawn(
            MeteorPreset::parse(value).ok_or(ParseError::MeteorUsage)?,
            Some(seed.parse().map_err(|_| ParseError::MeteorUsage)?),
        )),
        ("sky meteors spawn", _) => Err(ParseError::MeteorUsage),
        ("sky meteors debug light", []) => {
            Ok(ClientCommand::SkyMeteorsDebug(super::MeteorDebug::Light))
        }
        ("sky meteors debug light", ["on"]) => {
            Ok(ClientCommand::SkyMeteorsLight(super::MeteorLightDebug::On))
        }
        ("sky meteors debug light", ["off"]) => {
            Ok(ClientCommand::SkyMeteorsLight(super::MeteorLightDebug::Off))
        }
        ("sky meteors debug light", ["transmission"]) => Ok(ClientCommand::SkyMeteorsLight(
            super::MeteorLightDebug::Transmission,
        )),
        ("sky meteors debug light", _) => Err(ParseError::Usage(
            "sky meteors debug light on|off|transmission",
        )),
        ("sky meteors clear", []) => Ok(ClientCommand::SkyMeteorsClear),
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
        ("spawn_random_bolid", []) => Ok(ClientCommand::SpawnRandomBolid(1)),
        ("spawn_random_bolid", [value]) => {
            let count = value
                .parse::<u16>()
                .ok()
                .filter(|n| (1..=64).contains(n))
                .ok_or(ParseError::Usage("spawn_random_bolid [N: 1..64]"))?;
            Ok(ClientCommand::SpawnRandomBolid(count))
        }
        ("spawn_random_bolid", _) => Err(ParseError::Usage("spawn_random_bolid [N: 1..64]")),
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
