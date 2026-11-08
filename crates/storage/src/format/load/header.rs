use crate::{
    format::{parse_manifest, SAVE_VERSION},
    invalid_world_snapshot,
};
use minimium_world::world::{WorldInstanceId, WorldManifest, WorldMap};
use std::io;

pub(super) struct SnapshotHeader {
    pub(super) source_version: u8,
}

pub(super) fn parse_snapshot_header(
    header: &str,
    worlds: &[(WorldInstanceId, WorldMap, WorldManifest)],
) -> io::Result<SnapshotHeader> {
    let mut fields = header.split('\t');
    if fields.next() != Some("MINIMIUM_WORLD") {
        return Err(invalid_world_snapshot("invalid snapshot header"));
    }
    let version = fields
        .next()
        .and_then(|value| value.parse::<u8>().ok())
        .ok_or_else(|| invalid_world_snapshot("invalid snapshot version"))?;
    if version != SAVE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("snapshot version {version} is unsupported; use a separate data directory"),
        ));
    }
    let fingerprint = fields
        .next()
        .ok_or_else(|| invalid_world_snapshot("missing snapshot map fingerprint"))?;
    let fingerprint = u64::from_str_radix(fingerprint, 16)
        .map_err(|_| invalid_world_snapshot("invalid snapshot map fingerprint"))?;
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
    Ok(SnapshotHeader {
        source_version: version,
    })
}
