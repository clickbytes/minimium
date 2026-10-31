use super::{Location, WeatherSample};
use serde_json::Value;
use std::time::Duration;

const USER_AGENT: &str = "Minimium/0.1 github.com/clickbytes/minimium";

pub(super) fn fetch(location: Location) -> Result<(WeatherSample, String), String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .into();
    let mut response = agent
        .get(&location.url())
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| error.to_string())?;
    let document: Value = response
        .body_mut()
        .read_json()
        .map_err(|error| error.to_string())?;
    let forecast = document["properties"]["timeseries"]
        .as_array()
        .and_then(|series| series.first())
        .ok_or("empty MET Norway forecast")?;
    let details = &forecast["data"]["instant"]["details"];
    let percent = |name: &str| -> Result<f32, String> {
        let value = details[name]
            .as_f64()
            .ok_or_else(|| format!("missing MET Norway {name}"))?;
        if !value.is_finite() || !(0.0..=100.0).contains(&value) {
            return Err(format!("invalid MET Norway {name}"));
        }
        Ok(value as f32 / 100.0)
    };
    let low = percent("cloud_area_fraction_low")?;
    let middle = percent("cloud_area_fraction_medium")?;
    let high = percent("cloud_area_fraction_high")?;
    let rain = forecast["data"]["next_1_hours"]["details"]["precipitation_amount"]
        .as_f64()
        .unwrap_or(0.0);
    let speed = details["wind_speed"].as_f64().unwrap_or(0.0);
    let direction = details["wind_from_direction"].as_f64().unwrap_or(0.0);
    let temperature = details["air_temperature"].as_f64().unwrap_or(12.0);
    let humidity = details["relative_humidity"].as_f64().unwrap_or(60.0);
    if !rain.is_finite()
        || !(0.0..=200.0).contains(&rain)
        || !speed.is_finite()
        || !(0.0..=150.0).contains(&speed)
        || !direction.is_finite()
        || !temperature.is_finite()
        || !humidity.is_finite()
    {
        return Err("invalid MET Norway atmospheric data".to_owned());
    }
    let from = direction.to_radians();
    let humidity = (humidity as f32 / 100.0).clamp(0.0, 1.0);
    let fog =
        ((humidity - 0.87) / 0.13).clamp(0.0, 1.0) * ((5.0 - speed as f32) / 5.0).clamp(0.0, 1.0);
    let sample = WeatherSample::new(
        low,
        middle,
        high,
        rain as f32,
        (-speed * from.sin()) as f32,
        (-speed * from.cos()) as f32,
    )
    .with_atmosphere(
        humidity,
        temperature as f32,
        fog,
        (rain as f32 / 8.0).clamp(0.0, 1.0),
    );
    let time = forecast["time"]
        .as_str()
        .ok_or("missing MET Norway forecast time")?;
    Ok((sample, time.to_owned()))
}
