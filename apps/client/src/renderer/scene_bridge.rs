use super::{environment::PreparedEnvironmentFrame, SectorKey};
use minimium_core::world::{WorldInstanceId, WorldMap};

/// Immutable value and read-only world inputs consumed by the GPU renderer.
pub(super) struct RenderSceneSnapshot<'a> {
    pub(super) environment: &'a PreparedEnvironmentFrame,
    pub(super) map: Option<&'a WorldMap>,
    pub(super) player_position_m: [f64; 3],
    pub(super) instance: WorldInstanceId,
    pub(super) sector_hints: Vec<(SectorKey, u8)>,
}

impl<'a> RenderSceneSnapshot<'a> {
    pub(super) fn new(
        environment: &'a PreparedEnvironmentFrame,
        map: Option<&'a WorldMap>,
        player_position_m: [f64; 3],
        instance: WorldInstanceId,
        sector_hints: Vec<(SectorKey, u8)>,
    ) -> Self {
        Self {
            environment,
            map,
            player_position_m,
            instance,
            sector_hints,
        }
    }
}
