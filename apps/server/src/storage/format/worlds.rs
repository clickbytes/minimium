use super::*;
use std::io::Write;

pub(super) fn parse_saved_world(line: &str) -> io::Result<SavedWorldManifest> {
    let mut fields = line.split('\t');
    if fields.next() != Some("WORLD") {
        return Err(invalid_world_snapshot("invalid saved world record"));
    }
    let instance = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing saved world instance"))?
        .parse::<u128>()
        .ok()
        .and_then(WorldInstanceId::new)
        .ok_or_else(|| invalid_world_snapshot("invalid saved world instance"))?;
    let map_fingerprint = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing saved world fingerprint"))?;
    let map_fingerprint = u64::from_str_radix(map_fingerprint, 16)
        .map_err(|_| invalid_world_snapshot("invalid saved world fingerprint"))?;
    let manifest = parse_manifest(&mut fields)?;
    if fields.next().is_some() {
        return Err(invalid_world_snapshot(
            "saved world record has extra fields",
        ));
    }
    Ok(SavedWorldManifest {
        instance,
        map_fingerprint,
        manifest,
    })
}

pub(super) fn validate_saved_world(
    saved: &SavedWorldManifest,
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
) -> io::Result<()> {
    let Some((_, map, manifest)) = worlds
        .iter()
        .find(|(instance, _, _)| *instance == saved.instance)
    else {
        return Err(invalid_world_snapshot(
            "snapshot references an unloaded world instance",
        ));
    };
    if map.fingerprint() != saved.map_fingerprint || *manifest != saved.manifest {
        return Err(invalid_world_snapshot(
            "saved world manifest or fingerprint does not match",
        ));
    }
    Ok(())
}

pub(super) fn write_saved_world(
    file: &mut impl Write,
    world: &SavedWorldManifest,
) -> io::Result<()> {
    write!(
        file,
        "WORLD\t{}\t{:016x}",
        world.instance, world.map_fingerprint
    )?;
    write_manifest(file, world.manifest)
}

pub(super) fn write_manifest(file: &mut impl Write, manifest: WorldManifest) -> io::Result<()> {
    write!(
        file,
        "\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
        manifest.definition_id().get(),
        match manifest.topology() {
            WorldTopology::Planet => 0,
            WorldTopology::Dungeon => 1,
        },
        manifest.generation().seed().get(),
        manifest.generation().revision().get(),
        manifest.generation().position_quantum_mm(),
        manifest.content_revision().get(),
        hex_digest(manifest.generator_digest().as_bytes())
    )
}
