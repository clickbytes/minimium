use super::{ClientCommand, CommandRegistry, CommandSpec, ParseError};
use crate::environment::quality::SkyQuality;
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
        ("time status", []) => Ok(ClientCommand::TimeStatus),
        ("time scale", [value]) => {
            parse_scale_ppm(value).map_err(|_| ParseError::TimeScaleUsage)?;
            Ok(ClientCommand::TimeScale((*value).to_owned()))
        }
        ("time scale", _) => Err(ParseError::TimeScaleUsage),
        _ => Err(ParseError::UnknownCommand),
    }
}
