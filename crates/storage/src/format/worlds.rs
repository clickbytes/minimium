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
    )?;
    if manifest.generation().revision().get() >= 3 {
        if let Some(planet) = manifest.planet() {
            write!(
                file,
                "\t1\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                planet.mode,
                planet.radius_mm,
                planet.height_scale_permille,
                planet.sea_level_mm,
                planet.mountain_limit_mm,
                planet.detail_strength_mm,
                planet.detail_wavelength_mm,
                planet.mountain_band_frequency,
                hex_digest(&planet.package_digest)
            )?;
        } else {
            write!(file, "\t0")?;
        }
    }
    Ok(())
}

pub(super) fn parse_manifest(fields: &mut std::str::Split<'_, char>) -> io::Result<WorldManifest> {
    let definition = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing saved world definition"))?
        .parse::<u128>()
        .ok()
        .and_then(WorldDefinitionId::new)
        .ok_or_else(|| invalid_world_snapshot("invalid saved world definition"))?;
    let topology = match fields.next() {
        Some("0") => WorldTopology::Planet,
        Some("1") => WorldTopology::Dungeon,
        _ => return Err(invalid_world_snapshot("invalid saved world topology")),
    };
    let seed = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing saved world seed"))?
        .parse::<u128>()
        .map_err(|_| invalid_world_snapshot("invalid saved world seed"))?;
    let revision = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing saved generation revision"))?
        .parse::<u32>()
        .ok()
        .and_then(GenerationRevision::new)
        .ok_or_else(|| invalid_world_snapshot("invalid saved generation revision"))?;
    let quantum = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing saved position quantum"))?
        .parse::<u32>()
        .map_err(|_| invalid_world_snapshot("invalid saved position quantum"))?;
    let generation = GenerationSpec::new(WorldSeed::new(seed), revision, quantum)
        .ok_or_else(|| invalid_world_snapshot("invalid saved generation settings"))?;
    let content_revision = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing saved content revision"))?
        .parse::<u64>()
        .ok()
        .and_then(WorldContentRevision::new)
        .ok_or_else(|| invalid_world_snapshot("invalid saved content revision"))?;
    let digest = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing saved generator digest"))?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid_world_snapshot("invalid saved generator digest"));
    }
    let mut digest_bytes = [0_u8; 32];
    for (byte, pair) in digest_bytes
        .iter_mut()
        .zip(digest.as_bytes().as_chunks::<2>().0.iter())
    {
        let pair = std::str::from_utf8(pair)
            .map_err(|_| invalid_world_snapshot("invalid saved generator digest"))?;
        *byte = u8::from_str_radix(pair, 16)
            .map_err(|_| invalid_world_snapshot("invalid saved generator digest"))?;
    }
    let mut manifest = WorldManifest::new(
        definition,
        topology,
        generation,
        content_revision,
        GeneratorDigest::new(digest_bytes),
    );
    if revision.get() >= 3 {
        match fields.next() {
            Some("0") => {}
            Some("1") => {
                let number = |fields: &mut std::str::Split<'_, char>| -> io::Result<u64> {
                    next_planet_field(fields)?
                        .parse()
                        .map_err(|_| invalid_world_snapshot("invalid planet definition"))
                };
                let mode = u8::try_from(number(fields)?)
                    .map_err(|_| invalid_world_snapshot("invalid planet mode"))?;
                let radius_mm = number(fields)?;
                let height_scale_permille = u32::try_from(number(fields)?)
                    .map_err(|_| invalid_world_snapshot("invalid height scale"))?;
                let sea_level_mm = next_planet_field(fields)?
                    .parse()
                    .map_err(|_| invalid_world_snapshot("invalid sea level"))?;
                let mountain_limit_mm = u32::try_from(number(fields)?)
                    .map_err(|_| invalid_world_snapshot("invalid mountain limit"))?;
                let detail_strength_mm = u32::try_from(number(fields)?)
                    .map_err(|_| invalid_world_snapshot("invalid detail strength"))?;
                let detail_wavelength_mm = u32::try_from(number(fields)?)
                    .map_err(|_| invalid_world_snapshot("invalid detail wavelength"))?;
                let mountain_band_frequency = u32::try_from(number(fields)?)
                    .map_err(|_| invalid_world_snapshot("invalid mountain frequency"))?;
                let hex = next_planet_field(fields)?;
                if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return Err(invalid_world_snapshot("invalid planet package digest"));
                }
                let mut package_digest = [0; 32];
                for (byte, pair) in package_digest
                    .iter_mut()
                    .zip(hex.as_bytes().as_chunks::<2>().0.iter())
                {
                    *byte = u8::from_str_radix(
                        std::str::from_utf8(pair)
                            .map_err(|_| invalid_world_snapshot("invalid package digest"))?,
                        16,
                    )
                    .map_err(|_| invalid_world_snapshot("invalid package digest"))?;
                }
                let planet = PlanetDefinition {
                    mode,
                    radius_mm,
                    height_scale_permille,
                    sea_level_mm,
                    mountain_limit_mm,
                    detail_strength_mm,
                    detail_wavelength_mm,
                    mountain_band_frequency,
                    package_digest,
                };
                if !planet.is_valid() {
                    return Err(invalid_world_snapshot("invalid planet definition"));
                }
                manifest = manifest.with_planet(planet);
            }
            _ => return Err(invalid_world_snapshot("missing planet definition marker")),
        }
    }
    Ok(manifest)
}

fn next_planet_field<'a>(fields: &mut std::str::Split<'a, char>) -> io::Result<&'a str> {
    fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("incomplete planet definition"))
}
