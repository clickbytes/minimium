use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};
use minimium_environment::weather::CloudDebug;

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "terrain status",
        alias: None,
        usage: "terrain status",
        parser: parse,
    },
    CommandSpec {
        name: "atmosphere status",
        alias: None,
        usage: "atmosphere status",
        parser: parse,
    },
    CommandSpec {
        name: "atmosphere debug",
        alias: None,
        usage: "atmosphere debug off|transmission|scattering|optical_depth|distance|fog",
        parser: parse,
    },
    CommandSpec {
        name: "terrain debug",
        alias: None,
        usage: "terrain debug off|albedo|normals|depth|shadow|base_color|coverage|habitat|source",
        parser: parse,
    },
    CommandSpec {
        name: "weather status",
        alias: None,
        usage: "weather status",
        parser: parse,
    },
    CommandSpec {
        name: "weather clouds debug",
        alias: None,
        usage: "weather clouds debug off|regions|layers|wind|kinds|detail",
        parser: parse,
    },
    CommandSpec {
        name: "weather clouds detail",
        alias: None,
        usage: "weather clouds detail 2|3|4|5|6",
        parser: parse,
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    match words {
        ["terrain status", ""] => Ok(ClientCommand::TerrainStatus),
        ["atmosphere status", ""] => Ok(ClientCommand::AtmosphereStatus),
        ["atmosphere debug", mode] => {
            use minimium_renderer::atmosphere_debug::AtmosphereDebug as D;
            Ok(ClientCommand::AtmosphereDebug(match *mode {
                "off" => D::Off,
                "transmission" => D::Transmission,
                "scattering" => D::Scattering,
                "optical_depth" => D::OpticalDepth,
                "distance" => D::Distance,
                "fog" => D::Fog,
                _ => return Err(ParseError::UnknownCommand),
            }))
        }
        ["terrain debug", mode] => {
            use minimium_renderer::terrain_debug::TerrainDebug;
            Ok(ClientCommand::TerrainDebug(match *mode {
                "off" => TerrainDebug::Off,
                "albedo" => TerrainDebug::Albedo,
                "normals" => TerrainDebug::Normals,
                "depth" => TerrainDebug::Depth,
                "shadow" => TerrainDebug::Shadow,
                "base_color" => TerrainDebug::BaseColor,
                "coverage" => TerrainDebug::Coverage,
                "habitat" => TerrainDebug::Habitat,
                "source" => TerrainDebug::Source,
                _ => return Err(ParseError::UnknownCommand),
            }))
        }
        ["weather status", ""] => Ok(ClientCommand::WeatherStatus),
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
