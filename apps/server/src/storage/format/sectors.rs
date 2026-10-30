use super::*;
use minimium_core::world::{
    ObjectOriginId, PlanetFace, SectorKey, SectorRevision, WorldObjectState, WorldSectorState,
    MAX_OBJECT_STATE_BYTES, MAX_SECTOR_OBJECT_STATES,
};
use std::{io::Write, iter::Peekable, str::Lines};

pub(super) fn parse_saved_sector(
    line: &str,
    lines: &mut Peekable<Lines<'_>>,
) -> io::Result<(WorldInstanceId, WorldSectorState)> {
    let mut fields = line.split('\t');
    if fields.next() != Some("SECTOR") {
        return Err(invalid_world_snapshot("invalid saved sector record"));
    }
    let instance = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing sector instance"))?
        .parse::<u128>()
        .ok()
        .and_then(WorldInstanceId::new)
        .ok_or_else(|| invalid_world_snapshot("invalid sector instance"))?;
    let kind = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing sector topology"))?;
    let key = match kind {
        "P" => {
            let face = parse_planet_face(fields.next())?;
            let u = parse_u16(fields.next())?;
            let v = parse_u16(fields.next())?;
            SectorKey::planet(face, u, v)
                .ok_or_else(|| invalid_world_snapshot("invalid planet sector key"))?
        }
        "D" => SectorKey::dungeon(
            parse_i32(fields.next())?,
            parse_i32(fields.next())?,
            parse_i32(fields.next())?,
        ),
        _ => return Err(invalid_world_snapshot("invalid saved sector topology")),
    };
    let revision = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing sector revision"))?
        .parse::<u64>()
        .ok()
        .and_then(SectorRevision::new)
        .ok_or_else(|| invalid_world_snapshot("invalid sector revision"))?;
    let object_count = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing sector object count"))?
        .parse::<usize>()
        .map_err(|_| invalid_world_snapshot("invalid sector object count"))?;
    if object_count > MAX_SECTOR_OBJECT_STATES || fields.next().is_some() {
        return Err(invalid_world_snapshot(
            "invalid sector object count or fields",
        ));
    }
    let mut objects = Vec::with_capacity(object_count);
    for _ in 0..object_count {
        let object_line = lines
            .next()
            .ok_or_else(|| invalid_world_snapshot("truncated sector object records"))?;
        let mut object_fields = object_line.split('\t');
        if object_fields.next() != Some("OBJECT") {
            return Err(invalid_world_snapshot("invalid sector object record"));
        }
        let origin = object_fields
            .next()
            .ok_or_else(|| invalid_world_snapshot("missing object origin ID"))?
            .parse::<u128>()
            .ok()
            .and_then(ObjectOriginId::new)
            .ok_or_else(|| invalid_world_snapshot("invalid object origin ID"))?;
        let data = decode_object_state(object_fields.next())?;
        if object_fields.next().is_some() {
            return Err(invalid_world_snapshot(
                "sector object record has extra fields",
            ));
        }
        objects.push(
            WorldObjectState::new(origin, data)
                .ok_or_else(|| invalid_world_snapshot("object state exceeds size limit"))?,
        );
    }
    let state = WorldSectorState::new(key, revision, objects)
        .ok_or_else(|| invalid_world_snapshot("invalid or duplicate sector objects"))?;
    Ok((instance, state))
}

pub(super) fn write_saved_sector(
    file: &mut impl Write,
    instance: WorldInstanceId,
    state: &WorldSectorState,
) -> io::Result<()> {
    write!(file, "SECTOR\t{instance}")?;
    match state.key() {
        SectorKey::Planet(key) => {
            let (u, v) = key.coordinates();
            write!(file, "\tP\t{}\t{u}\t{v}", key.face().index())?;
        }
        SectorKey::Dungeon(key) => {
            let (x, y, z) = key.coordinates();
            write!(file, "\tD\t{x}\t{y}\t{z}")?;
        }
    }
    writeln!(
        file,
        "\t{}\t{}",
        state.revision().get(),
        state.objects().len()
    )?;
    for object in state.objects() {
        write!(file, "OBJECT\t{}\t", object.origin())?;
        for byte in object.data() {
            write!(file, "{byte:02x}")?;
        }
        writeln!(file)?;
    }
    Ok(())
}

fn parse_planet_face(value: Option<&str>) -> io::Result<PlanetFace> {
    match value.and_then(|value| value.parse::<u8>().ok()) {
        Some(0) => Ok(PlanetFace::PositiveX),
        Some(1) => Ok(PlanetFace::NegativeX),
        Some(2) => Ok(PlanetFace::PositiveY),
        Some(3) => Ok(PlanetFace::NegativeY),
        Some(4) => Ok(PlanetFace::PositiveZ),
        Some(5) => Ok(PlanetFace::NegativeZ),
        _ => Err(invalid_world_snapshot("invalid planet face index")),
    }
}

fn parse_u16(value: Option<&str>) -> io::Result<u16> {
    value
        .ok_or_else(|| invalid_world_snapshot("missing planet sector coordinate"))?
        .parse::<u16>()
        .map_err(|_| invalid_world_snapshot("invalid planet sector coordinate"))
}

fn parse_i32(value: Option<&str>) -> io::Result<i32> {
    value
        .ok_or_else(|| invalid_world_snapshot("missing dungeon sector coordinate"))?
        .parse::<i32>()
        .map_err(|_| invalid_world_snapshot("invalid dungeon sector coordinate"))
}

fn decode_object_state(value: Option<&str>) -> io::Result<Vec<u8>> {
    let encoded = value.ok_or_else(|| invalid_world_snapshot("missing object state"))?;
    if encoded.len() > MAX_OBJECT_STATE_BYTES * 2
        || encoded.len() % 2 != 0
        || !encoded.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid_world_snapshot("invalid object state encoding"));
    }
    encoded
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| {
            let pair = std::str::from_utf8(pair)
                .map_err(|_| invalid_world_snapshot("invalid object state encoding"))?;
            u8::from_str_radix(pair, 16)
                .map_err(|_| invalid_world_snapshot("invalid object state encoding"))
        })
        .collect()
}
