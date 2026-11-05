use super::environment::PreparedEnvironmentFrame;
use minimium_world::world::{SectorKey, WorldMap};

/// Immutable value and read-only world inputs consumed by the GPU renderer.
pub(super) struct RenderSceneSnapshot<'a> {
    pub(super) environment: &'a PreparedEnvironmentFrame,
    pub(super) map: Option<&'a WorldMap>,
    pub(super) sector_hints: Vec<(SectorKey, u8)>,
}

impl<'a> RenderSceneSnapshot<'a> {
    pub(super) fn new(
        environment: &'a PreparedEnvironmentFrame,
        map: Option<&'a WorldMap>,
        sector_hints: Vec<(SectorKey, u8)>,
    ) -> Self {
        Self {
            environment,
            map,
            sector_hints,
        }
    }
}
