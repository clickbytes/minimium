use super::{
    ArgumentSpec as Arg, ArgumentType as ArgType, ClientCommand, CommandRegistry, CommandSpec,
    ParseError,
};

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "set_altitude",
        alias: None,
        usage: "set_altitude <0..10000 km> - height above local ground; requires fly on",
        arguments: &[Arg::new(
            "altitude",
            ArgType::Decimal,
            false,
            false,
            None,
            Some("0..=10000"),
            Some("km"),
            "height above local ground",
        )],
        parser: parse,
    },
    CommandSpec {
        name: "world portal",
        alias: Some("portal"),
        usage: "world portal",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "world teleport",
        alias: Some("map_telepo"),
        usage: "world teleport: click land on the Earth overview",
        arguments: &[],
        parser: parse,
    },
    CommandSpec {
        name: "dev goto",
        alias: None,
        usage: "dev goto LATITUDE LONGITUDE (-90..90, -180..180 degrees)",
        arguments: &[
            Arg::new(
                "latitude",
                ArgType::Decimal,
                false,
                false,
                None,
                Some("-90..=90"),
                Some("degrees"),
                "geographic latitude",
            ),
            Arg::new(
                "longitude",
                ArgType::Decimal,
                false,
                false,
                None,
                Some("-180..=180"),
                Some("degrees"),
                "geographic longitude",
            ),
        ],
        parser: parse,
    },
];

pub(super) fn register(registry: &mut CommandRegistry) {
    registry.register(COMMANDS);
}

fn parse(words: &[&str]) -> Result<ClientCommand, ParseError> {
    match words {
        ["world portal", raw] if raw.is_empty() => return Ok(ClientCommand::InteractPortal),
        ["world teleport", raw] if raw.is_empty() => return Ok(ClientCommand::MapTeleport),
        _ => {}
    }
    if let ["set_altitude", raw] = words {
        let km = raw
            .parse::<f32>()
            .map_err(|_| ParseError::InvalidCoordinate)?;
        return if km.is_finite() && (0.0..=10000.0).contains(&km) {
            Ok(ClientCommand::SetAltitude(km))
        } else {
            Err(ParseError::InvalidCoordinate)
        };
    }
    let ["dev goto", raw] = words else {
        return Err(ParseError::UnknownCommand);
    };
    let parts = raw.split_whitespace().collect::<Vec<_>>();
    let [latitude, longitude] = parts.as_slice() else {
        return Err(ParseError::UnknownCommand);
    };
    let latitude = latitude
        .parse::<f32>()
        .map_err(|_| ParseError::InvalidCoordinate)?;
    let longitude = longitude
        .parse::<f32>()
        .map_err(|_| ParseError::InvalidCoordinate)?;
    if !latitude.is_finite()
        || !longitude.is_finite()
        || !(-90.0..=90.0).contains(&latitude)
        || !(-180.0..=180.0).contains(&longitude)
    {
        return Err(ParseError::InvalidCoordinate);
    }
    Ok(ClientCommand::Goto {
        latitude,
        longitude,
    })
}

pub(crate) fn direction(latitude: f32, longitude: f32) -> [f32; 3] {
    let latitude = latitude.to_radians();
    let longitude = longitude.to_radians();
    [
        latitude.cos() * longitude.cos(),
        latitude.sin(),
        latitude.cos() * longitude.sin(),
    ]
}

pub(crate) fn nearest_portal_entity(
    replicas: &minimium_client_runtime::ReplicatedEntities,
    position: [f32; 3],
) -> Option<minimium_world::entity::EntityId> {
    replicas
        .entities
        .iter()
        .filter_map(|(entity, replica)| {
            let component = replica
                .components
                .get(&minimium_game_sample::PORTAL_COMPONENT)?;
            if component.schema != minimium_game_sample::PORTAL_SCHEMA {
                return None;
            }
            let portal = minimium_game_sample::PortalComponent::decode(&component.bytes)?;
            let distance_squared = portal
                .position
                .iter()
                .zip(position)
                .map(|(portal, player)| (*portal - player).powi(2))
                .sum::<f32>();
            (distance_squared <= 9.0).then_some((*entity, distance_squared))
        })
        .min_by(|(left_id, left_distance), (right_id, right_distance)| {
            left_distance
                .total_cmp(right_distance)
                .then_with(|| left_id.cmp(right_id))
        })
        .map(|(entity, _)| entity)
}
