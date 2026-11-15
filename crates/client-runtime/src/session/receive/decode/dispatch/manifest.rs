use super::*;

pub(super) fn manifest_matches_content(
    manifest: &minimium_world::world::WorldManifest,
    fingerprint: u64,
) -> bool {
    if minimium_procgen::validate_world_manifest(*manifest).is_err() {
        return false;
    }
    match manifest.source() {
        minimium_world::world::WorldSource::Authored => {
            manifest.generator_digest()
                == minimium_world::world::GeneratorDigest::authored_map_fingerprint(fingerprint)
        }
        minimium_world::world::WorldSource::Generated => manifest_matches_generator(manifest),
    }
}
