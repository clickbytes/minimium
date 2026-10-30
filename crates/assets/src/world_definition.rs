use minimium_core::world::{
    GenerationRevision, GenerationSpec, GeneratorDigest, ObjectOriginId, WorldContentRevision,
    WorldDefinitionId, WorldManifest, WorldSeed, WorldTopology,
};
use minimium_procgen::{generator_digest, DungeonBlueprint, DungeonRoom, DungeonTile};
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

const MAX_DEFINITION_BYTES: usize = 1024 * 1024;
const MAX_DUNGEON_DIMENSION: u16 = 128;
const MAX_DUNGEON_TILES: usize = MAX_DUNGEON_DIMENSION as usize * MAX_DUNGEON_DIMENSION as usize;

#[derive(Clone, Debug)]
pub struct WorldDefinition {
    manifest: WorldManifest,
    dungeon: DungeonBlueprint,
}

impl WorldDefinition {
    pub const fn manifest(&self) -> WorldManifest {
        self.manifest
    }

    pub fn dungeon(&self) -> &DungeonBlueprint {
        &self.dungeon
    }
}

pub fn load_world_definition(path: &Path) -> io::Result<WorldDefinition> {
    let mut source = String::new();
    File::open(path)?
        .take((MAX_DEFINITION_BYTES + 1) as u64)
        .read_to_string(&mut source)?;
    parse_world_definition(&source)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Parses a manifest and a manually authored dungeon into the same validated
/// blueprint consumed by the procedural materializer.
pub fn parse_world_definition(source: &str) -> Result<WorldDefinition, String> {
    if source.len() > MAX_DEFINITION_BYTES {
        return Err(format!(
            "world definition exceeds {MAX_DEFINITION_BYTES} bytes"
        ));
    }
    let mut lines = source.lines().enumerate().filter_map(|(index, line)| {
        let content = line.split('#').next().unwrap_or("").trim();
        (!content.is_empty()).then_some((index + 1, content))
    });
    let Some((line_number, header)) = lines.next() else {
        return Err("world definition is empty".into());
    };
    if header != "MINIMIUM_WORLD_DEFINITION 1" {
        return Err(format!(
            "line {line_number}: expected `MINIMIUM_WORLD_DEFINITION 1`"
        ));
    }
    let Some((line_number, manifest_line)) = lines.next() else {
        return Err("world definition is missing a manifest".into());
    };
    let manifest = parse_manifest(line_number, manifest_line)?;
    let mut dimensions = None;
    let mut tiles = Vec::new();
    let mut tile_seen = Vec::new();
    let mut rooms = Vec::new();
    let mut record_count = 0usize;
    for (line_number, line) in lines {
        record_count += 1;
        if record_count > MAX_DUNGEON_TILES + 64 {
            return Err("world definition has too many records".into());
        }
        let mut fields = line.split_ascii_whitespace();
        match fields.next() {
            Some("dungeon") if dimensions.is_none() => {
                let width = parse_u16(fields.next(), line_number, "dungeon width")?;
                let height = parse_u16(fields.next(), line_number, "dungeon height")?;
                if fields.next().is_some()
                    || !(3..=MAX_DUNGEON_DIMENSION).contains(&width)
                    || !(3..=MAX_DUNGEON_DIMENSION).contains(&height)
                {
                    return Err(format!("line {line_number}: invalid dungeon dimensions"));
                }
                let tile_count = usize::from(width) * usize::from(height);
                dimensions = Some((width, height));
                tiles.resize(tile_count, DungeonTile::Void);
                tile_seen.resize(tile_count, false);
            }
            Some("room") if dimensions.is_some() => {
                if rooms.len() >= 32 {
                    return Err(format!("line {line_number}: too many dungeon rooms"));
                }
                let origin =
                    ObjectOriginId::new(parse_u128(fields.next(), line_number, "room origin ID")?)
                        .ok_or_else(|| format!("line {line_number}: invalid room origin ID"))?;
                let x = parse_u16(fields.next(), line_number, "room x")?;
                let y = parse_u16(fields.next(), line_number, "room y")?;
                let width = parse_u16(fields.next(), line_number, "room width")?;
                let height = parse_u16(fields.next(), line_number, "room height")?;
                let center_x = parse_u16(fields.next(), line_number, "room center x")?;
                let center_y = parse_u16(fields.next(), line_number, "room center y")?;
                if fields.next().is_some()
                    || rooms
                        .iter()
                        .any(|room: &DungeonRoom| room.origin_id == origin)
                {
                    return Err(format!(
                        "line {line_number}: invalid or duplicate room record"
                    ));
                }
                rooms.push(DungeonRoom {
                    origin_id: origin,
                    x,
                    y,
                    width,
                    height,
                    center: [center_x, center_y],
                });
            }
            Some("tile") if dimensions.is_some() => {
                let (width, _) = dimensions.expect("dimension guard established the width");
                let x = parse_u16(fields.next(), line_number, "tile x")?;
                let y = parse_u16(fields.next(), line_number, "tile y")?;
                let tile = match fields.next() {
                    Some("void") => DungeonTile::Void,
                    Some("room") => DungeonTile::Room,
                    Some("corridor") => DungeonTile::Corridor,
                    _ => return Err(format!("line {line_number}: invalid dungeon tile")),
                };
                if fields.next().is_some()
                    || x >= width
                    || usize::from(y) * usize::from(width) + usize::from(x) >= tiles.len()
                {
                    return Err(format!(
                        "line {line_number}: tile is out of bounds or malformed"
                    ));
                }
                let index = usize::from(y) * usize::from(width) + usize::from(x);
                if tile_seen[index] {
                    return Err(format!("line {line_number}: duplicate dungeon tile"));
                }
                tiles[index] = tile;
                tile_seen[index] = true;
            }
            _ => {
                return Err(format!(
                    "line {line_number}: unknown, duplicate, or out-of-order world record"
                ))
            }
        }
    }
    let (width, height) = dimensions.ok_or("world definition is missing dungeon dimensions")?;
    if tiles.len() != usize::from(width) * usize::from(height)
        || tiles.len() > MAX_DUNGEON_TILES
        || tile_seen.iter().any(|seen| !seen)
    {
        return Err("world definition must describe every dungeon tile".into());
    }
    let dungeon = DungeonBlueprint::from_parts(width, height, tiles, rooms)
        .ok_or_else(|| "dungeon rooms or walkable connectivity are invalid".to_owned())?;
    Ok(WorldDefinition { manifest, dungeon })
}

fn parse_manifest(line_number: usize, line: &str) -> Result<WorldManifest, String> {
    let mut fields = line.split_ascii_whitespace();
    if fields.next() != Some("manifest") {
        return Err(format!("line {line_number}: expected a manifest record"));
    }
    let definition =
        WorldDefinitionId::new(parse_u128(fields.next(), line_number, "definition ID")?)
            .ok_or_else(|| format!("line {line_number}: invalid definition ID"))?;
    if fields.next() != Some("dungeon") {
        return Err(format!(
            "line {line_number}: manual world topology must be `dungeon`"
        ));
    }
    let seed = parse_u128(fields.next(), line_number, "world seed")?;
    let generation_revision = parse_u32(fields.next(), line_number, "generation revision")?;
    let generation_revision = GenerationRevision::new(generation_revision)
        .ok_or_else(|| format!("line {line_number}: generation revision cannot be zero"))?;
    let quantum = parse_u32(fields.next(), line_number, "position quantum")?;
    let generation = GenerationSpec::new(WorldSeed::new(seed), generation_revision, quantum)
        .ok_or_else(|| format!("line {line_number}: invalid position quantum"))?;
    let content_revision = parse_u64(fields.next(), line_number, "content revision")?;
    let content_revision = WorldContentRevision::new(content_revision)
        .ok_or_else(|| format!("line {line_number}: content revision cannot be zero"))?;
    let digest = fields
        .next()
        .ok_or_else(|| format!("line {line_number}: missing generator digest"))?;
    if fields.next().is_some()
        || digest.len() != 64
        || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(format!("line {line_number}: invalid manifest record"));
    }
    let mut digest_bytes = [0_u8; 32];
    for (byte, pair) in digest_bytes
        .iter_mut()
        .zip(digest.as_bytes().as_chunks::<2>().0.iter())
    {
        let pair = std::str::from_utf8(pair)
            .map_err(|_| format!("line {line_number}: invalid generator digest"))?;
        *byte = u8::from_str_radix(pair, 16)
            .map_err(|_| format!("line {line_number}: invalid generator digest"))?;
    }
    let digest = GeneratorDigest::new(digest_bytes);
    let manifest = WorldManifest::new(
        definition,
        WorldTopology::Dungeon,
        generation,
        content_revision,
        digest,
    );
    if digest != generator_digest(WorldTopology::Dungeon, generation) {
        return Err(format!(
            "line {line_number}: digest does not match the dungeon generator contract"
        ));
    }
    Ok(manifest)
}

fn parse_u16(value: Option<&str>, line: usize, label: &str) -> Result<u16, String> {
    value
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| format!("line {line}: invalid {label}"))
}

fn parse_u32(value: Option<&str>, line: usize, label: &str) -> Result<u32, String> {
    value
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| format!("line {line}: invalid {label}"))
}

fn parse_u64(value: Option<&str>, line: usize, label: &str) -> Result<u64, String> {
    value
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| format!("line {line}: invalid {label}"))
}

fn parse_u128(value: Option<&str>, line: usize, label: &str) -> Result<u128, String> {
    value
        .and_then(|text| text.parse().ok())
        .ok_or_else(|| format!("line {line}: invalid {label}"))
}
