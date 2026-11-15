use crate::{
    format::{parse_manifest, SAVE_VERSION},
    invalid_world_snapshot,
};
use minimium_world::world::{WorldInstanceId, WorldManifest, WorldMap};
use std::{
    fs::File,
    io::{self, Read},
    path::Path,
};

pub(super) struct SnapshotHeader {
    pub(super) source_version: u8,
}

pub(super) fn preflight_snapshot_version(path: &Path) -> io::Result<()> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut prefix = Vec::with_capacity(64);
    file.take(64).read_to_end(&mut prefix)?;
    let mut fields = prefix.split(|byte| *byte == b'\t');
    if fields.next() != Some(b"MINIMIUM_WORLD".as_slice()) {
        return Err(invalid_world_snapshot("invalid snapshot header"));
    }
    let version = fields
        .next()
        .and_then(|field| std::str::from_utf8(field).ok())
        .and_then(|field| field.parse::<u8>().ok())
        .ok_or_else(|| invalid_world_snapshot("invalid snapshot version"))?;
    if version != SAVE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("snapshot version {version} is unsupported; use a separate data directory"),
        ));
    }
    Ok(())
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
