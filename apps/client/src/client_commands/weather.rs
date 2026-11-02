use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};
use crate::environment::weather::CloudDebug;

const COMMANDS: &[CommandSpec] = &[
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
