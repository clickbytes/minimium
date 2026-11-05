//! Immutable, GPU-independent client state prepared for presentation adapters.
use minimium_simulation::CharacterState;
use minimium_world::world::{Position3Mm, WorldInstanceId, WorldManifest};

/// Read-only gameplay/session image consumed by the client presentation adapter.
pub struct ClientView<'a> {
    pub world_instance: WorldInstanceId,
    pub manifest: Option<WorldManifest>,
    pub character: CharacterState,
    pub position_mm: Option<Position3Mm>,
    pub player_position_m: [f64; 3],
    pub health: u16,
    pub remote_positions_mm: &'a [[i64; 3]],
    pub sectors_ready: bool,
}

impl<'a> ClientView<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        world_instance: WorldInstanceId,
        manifest: Option<WorldManifest>,
        character: CharacterState,
        position_mm: Option<Position3Mm>,
        health: u16,
        remote_positions_mm: &'a [[i64; 3]],
        sectors_ready: bool,
    ) -> Self {
        let player_position_m = position_mm.map_or_else(
            || character.position.map(f64::from),
            |position| position.0.map(|value| value as f64 / 1000.0),
        );
        Self {
            world_instance,
            manifest,
            character,
            position_mm,
            player_position_m,
            health,
            remote_positions_mm,
            sectors_ready,
        }
    }
}
