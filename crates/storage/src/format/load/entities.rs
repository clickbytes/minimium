use super::super::*;
use std::collections::HashSet;

pub(super) fn parse_entity_counter(
    line: &str,
    worlds: &HashSet<WorldInstanceId>,
) -> io::Result<(WorldInstanceId, u64)> {
    let mut fields = line.split('\t');
    if fields.next() != Some("ENTITY_COUNTER") {
        return Err(invalid_world_snapshot("invalid entity counter"));
    }
    let world = parse_instance(fields.next())?;
    let next_id = parse_u64(fields.next())?;
    if fields.next().is_some() || !worlds.contains(&world) {
        return Err(invalid_world_snapshot("invalid entity counter fields"));
    }
    Ok((world, next_id))
}

pub(super) fn parse_world_initialization(
    line: &str,
    worlds: &HashSet<WorldInstanceId>,
) -> io::Result<(WorldInstanceId, bool)> {
    let mut fields = line.split('\t');
    if fields.next() != Some("WORLD_INIT") {
        return Err(invalid_world_snapshot(
            "invalid world initialization marker",
        ));
    }
    let world = parse_instance(fields.next())?;
    let initialized = match fields.next() {
        Some("0") => false,
        Some("1") => true,
        _ => return Err(invalid_world_snapshot("invalid world initialization state")),
    };
    if fields.next().is_some() || !worlds.contains(&world) {
        return Err(invalid_world_snapshot(
            "invalid world initialization marker fields",
        ));
    }
    Ok((world, initialized))
}

pub(super) fn parse_entity(
    line: &str,
    worlds: &HashSet<WorldInstanceId>,
) -> io::Result<(WorldInstanceId, minimium_world::entity::EntityId)> {
    let mut fields = line.split('\t');
    if fields.next() != Some("ENTITY") {
        return Err(invalid_world_snapshot("invalid entity record"));
    }
    let world = parse_instance(fields.next())?;
    let entity = minimium_world::entity::EntityId::new(parse_u64(fields.next())?)
        .ok_or_else(|| invalid_world_snapshot("invalid entity ID"))?;
    if fields.next().is_some() || !worlds.contains(&world) {
        return Err(invalid_world_snapshot("invalid entity record fields"));
    }
    Ok((world, entity))
}

pub(super) fn parse_component(
    line: &str,
) -> io::Result<(
    WorldInstanceId,
    minimium_world::entity::EntityId,
    crate::PersistedComponent,
)> {
    let mut fields = line.split('\t');
    if fields.next() != Some("COMPONENT") {
        return Err(invalid_world_snapshot("invalid component record"));
    }
    let world = parse_instance(fields.next())?;
    let entity = minimium_world::entity::EntityId::new(parse_u64(fields.next())?)
        .ok_or_else(|| invalid_world_snapshot("invalid component entity ID"))?;
    let kind = minimium_world::entity::ComponentTypeId::new(parse_u32(fields.next())?)
        .ok_or_else(|| invalid_world_snapshot("invalid component type"))?;
    let schema = minimium_world::entity::SchemaVersion::new(parse_u32(fields.next())?)
        .ok_or_else(|| invalid_world_snapshot("invalid component schema"))?;
    let encoded = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing component payload"))?;
    if fields.next().is_some()
        || encoded.len() % 2 != 0
        || encoded.len() / 2 > minimium_world::entity::MAX_COMPONENT_BYTES
        || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid_world_snapshot("invalid component payload"));
    }
    let bytes = (0..encoded.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&encoded[index..index + 2], 16)
                .map_err(|_| invalid_world_snapshot("invalid component payload"))
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok((
        world,
        entity,
        crate::PersistedComponent {
            kind,
            schema,
            bytes,
        },
    ))
}

fn parse_instance(value: Option<&str>) -> io::Result<WorldInstanceId> {
    value
        .ok_or_else(|| invalid_world_snapshot("missing world instance"))?
        .parse::<u128>()
        .ok()
        .and_then(WorldInstanceId::new)
        .ok_or_else(|| invalid_world_snapshot("invalid world instance"))
}

fn parse_u64(value: Option<&str>) -> io::Result<u64> {
    value
        .ok_or_else(|| invalid_world_snapshot("missing integer"))?
        .parse()
        .map_err(|_| invalid_world_snapshot("invalid integer"))
}

fn parse_u32(value: Option<&str>) -> io::Result<u32> {
    value
        .ok_or_else(|| invalid_world_snapshot("missing integer"))?
        .parse()
        .map_err(|_| invalid_world_snapshot("invalid integer"))
}
