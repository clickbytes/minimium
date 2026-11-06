use crate::{format::worlds::parse_manifest, invalid_world_snapshot};
use minimium_world::world::{WorldInstanceId, WorldManifest, WorldMap};
use std::io;

pub(super) struct SnapshotHeader {
    pub(super) source_version: u8,
    pub(super) legacy_world: WorldInstanceId,
}

pub(super) fn parse_snapshot_header(
    header: &str,
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
) -> io::Result<SnapshotHeader> {
    let mut fields = header.split('\t');
    let signature = fields.next();
    let version = fields.next();
    let fingerprint = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing snapshot map fingerprint"))?;
    let fingerprint = u64::from_str_radix(fingerprint, 16)
        .map_err(|_| invalid_world_snapshot("invalid snapshot map fingerprint"))?;
    if signature != Some("MINIMIUM_WORLD") {
        return Err(invalid_world_snapshot("invalid snapshot header"));
    }
    if version == Some("1") {
        return Err(invalid_world_snapshot("snapshot version 1 has no client profile IDs; back up or remove the old snapshot before starting"));
    }
    if !matches!(version, Some("2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")) {
        return Err(invalid_world_snapshot("unsupported snapshot version"));
    }
    let source_version = version
        .and_then(|value| value.parse::<u8>().ok())
        .ok_or_else(|| invalid_world_snapshot("invalid snapshot version"))?;
    let legacy_world = worlds
        .iter()
        .find(|(_, map, _)| map.fingerprint() == fingerprint)
        .map(|(instance, _, _)| *instance)
        .ok_or_else(|| {
            invalid_world_snapshot(
                "snapshot map fingerprint does not match any loaded legacy world",
            )
        })?;
    match version {
        Some("4" | "5" | "6" | "7" | "8" | "9") => {
            let saved_manifest = parse_manifest(&mut fields)?;
            let Some((_, primary_map, primary_manifest)) = worlds.first() else {
                return Err(invalid_world_snapshot("no primary world is loaded"));
            };
            if fields.next().is_some()
                || primary_map.fingerprint() != fingerprint
                || *primary_manifest != saved_manifest
            {
                return Err(invalid_world_snapshot(
                    "snapshot primary world manifest or fingerprint does not match",
                ));
            }
        }
        Some("2" | "3") if fields.next().is_some() => {
            return Err(invalid_world_snapshot("invalid legacy snapshot header"));
        }
        Some("2" | "3") => {}
        _ => return Err(invalid_world_snapshot("invalid snapshot header")),
    }
    Ok(SnapshotHeader {
        source_version,
        legacy_world,
    })
}
