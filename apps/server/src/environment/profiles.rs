use minimium_core::{
    environment::EnvironmentProfile,
    world::{WorldManifest, WorldTopology},
};

pub(crate) fn profile_for_manifest(manifest: WorldManifest) -> EnvironmentProfile {
    match manifest.topology() {
        WorldTopology::Dungeon => EnvironmentProfile::Interior,
        WorldTopology::Planet if manifest.planet().is_some_and(|planet| planet.mode == 1) => {
            EnvironmentProfile::Earth
        }
        WorldTopology::Planet => EnvironmentProfile::EarthLike,
    }
}
