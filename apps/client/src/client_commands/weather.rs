use super::{
    ArgumentSpec as Arg, ArgumentType as ArgType, ClientCommand, CommandRegistry, CommandSpec,
    ParseError,
};
use minimium_renderer::cloud_presentation::CloudDebug;

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "terrain status",
        alias: None,
        usage: "terrain status",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "atmosphere status",
        alias: None,
        usage: "atmosphere status",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "atmosphere debug",
        alias: None,
        usage: "atmosphere debug off|transmission|scattering|optical_depth|distance|fog|fog_shadow",
        arguments: &[Arg::new(
            "mode",
            ArgType::Enumeration,
            false,
            false,
            Some(&[
                "off",
                "transmission",
                "scattering",
                "optical_depth",
                "distance",
                "fog",
                "fog_shadow",
            ]),
            None,
            None,
            "atmosphere visualization mode",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "terrain debug",
        alias: None,
        usage: "terrain debug off|albedo|normals|depth|shadow|base_color|coverage|habitat|source|moon_direct|moon_diffuse|background|moon_coverage|light_channels",
        arguments: &[Arg::new(
            "mode",
            ArgType::Enumeration,
            false,
            false,
            Some(&[
                "off",
                "albedo",
                "normals",
                "depth",
                "shadow",
                "base_color",
                "coverage",
                "habitat",
                "source",
                "moon_direct",
                "moon_diffuse",
                "background",
                "moon_coverage",
                "light_channels",
            ]),
            None,
            None,
            "terrain visualization mode",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "weather status",
        alias: None,
        usage: "weather status",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "weather clouds debug",
        alias: None,
        usage: "weather clouds debug off|regions|layers|wind|kinds|detail",
        arguments: &[Arg::new(
            "mode",
            ArgType::Enumeration,
            false,
            false,
            Some(&["off", "regions", "layers", "wind", "kinds", "detail"]),
            None,
            None,
            "cloud debug visualization mode",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "weather clouds detail",
        alias: None,
        usage: "weather clouds detail 2|3|4|5|6",
        arguments: &[Arg::new(
            "detail",
            ArgType::Integer,
            false,
            false,
            Some(&["2", "3", "4", "5", "6"]),
            Some("2..=6"),
            None,
            "cloud rendering detail",
        )],
        parser: parse,
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    match words {
        ["terrain status", ""] => Ok(ClientCommand::Presentation(
            super::PresentationOperation::TerrainStatus,
        )),
        ["atmosphere status", ""] => Ok(ClientCommand::Presentation(
            super::PresentationOperation::AtmosphereStatus,
        )),
        ["atmosphere debug", mode] => super::properties::parse_atmosphere_debug(mode)
            .map(ClientCommand::AtmosphereDebug)
            .ok_or(ParseError::UnknownCommand),
        ["terrain debug", mode] => super::properties::parse_terrain_debug(mode)
            .map(ClientCommand::TerrainDebug)
            .ok_or(ParseError::UnknownCommand),
        ["weather status", ""] => Ok(ClientCommand::Presentation(
            super::PresentationOperation::WeatherStatus,
        )),
        ["weather clouds detail", level] => {
            let level = level
                .parse::<u8>()
                .map_err(|_| ParseError::UnknownCommand)?;
            if !(2..=6).contains(&level) {
                return Err(ParseError::UnknownCommand);
            }
            Ok(ClientCommand::WeatherDetail(level))
        }
        ["weather clouds debug", mode] => {
            let mode = match *mode {
                "off" => CloudDebug::Off,
                "regions" => CloudDebug::Regions,
                "layers" => CloudDebug::Layers,
                "wind" => CloudDebug::Wind,
                "kinds" => CloudDebug::Kinds,
                "detail" => CloudDebug::Detail,
                _ => return Err(ParseError::UnknownCommand),
            };
            Ok(ClientCommand::WeatherDebug(mode))
        }
        _ => Err(ParseError::UnknownCommand),
    }
}
