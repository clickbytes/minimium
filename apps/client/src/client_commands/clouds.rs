use super::{ClientCommand, ParseError};
use crate::environment::weather::WeatherCommand;
use crate::renderer::cloud_settings::{CloudCommand, CloudDebug, CloudLighting, CloudMethod, CloudQuality};

pub(super) fn parse(arguments: &[&str]) -> Result<ClientCommand, ParseError> {
    let command = match arguments {
        [] | ["status"] => CloudCommand::Status,
        ["reset"] => CloudCommand::Reset,
        ["quality", value] => CloudCommand::Quality(match *value {
            "sky" => CloudQuality::Sky,
            "low" => CloudQuality::Low,
            "standard" => CloudQuality::Standard,
            "high" => CloudQuality::High,
            _ => return Err(ParseError::WeatherUsage),
        }),
        ["method", value] => CloudCommand::Method(match *value {
            "analytic" => CloudMethod::Analytic,
            "volumetric" => CloudMethod::Volumetric,
            _ => return Err(ParseError::WeatherUsage),
        }),
        ["lighting", value] => CloudCommand::Lighting(match *value {
            "low" => CloudLighting::Low,
            "standard" => CloudLighting::Standard,
            "high" => CloudLighting::High,
            _ => return Err(ParseError::WeatherUsage),
        }),
        ["debug", value] => CloudCommand::Debug(match *value {
            "off" => CloudDebug::Off,
            "bounds" => CloudDebug::Bounds,
            "ids" => CloudDebug::Ids,
            "density" => CloudDebug::Density,
            "lod" => CloudDebug::Lod,
            _ => return Err(ParseError::WeatherUsage),
        }),
        ["scale", value] => CloudCommand::Scale(match *value {
            "0.25" => 1,
            "0.5" => 2,
            "0.75" => 3,
            "1" => 4,
            _ => return Err(ParseError::WeatherUsage),
        }),
        ["instances", value] => CloudCommand::Instances(integer(value, 512, 32768)?),
        ["lod", value] => CloudCommand::Lod(integer(value, 8, 128)?),
        _ => return Err(ParseError::WeatherUsage),
    };
    Ok(ClientCommand::Weather(WeatherCommand::Clouds(command)))
}

fn integer(value: &str, min: u32, max: u32) -> Result<u32, ParseError> {
    value
        .parse::<u32>()
        .ok()
        .filter(|v| (min..=max).contains(v))
        .ok_or(ParseError::WeatherUsage)
}
