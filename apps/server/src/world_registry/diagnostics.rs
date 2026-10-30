use super::*;

impl WorldRegistry {
    /// Number of connected players across every running world instance.
    pub(crate) fn total_players(&self) -> usize {
        self.instances
            .values()
            .map(|instance| instance.world.player_count())
            .sum()
    }

    /// Player snapshots are tagged with their owning world instance.
    pub(crate) fn player_snapshots(&self) -> Vec<(WorldInstanceId, minimium_core::world::Player)> {
        self.iter()
            .flat_map(|(instance_id, instance)| {
                instance
                    .world
                    .snapshot()
                    .into_iter()
                    .map(move |player| (instance_id, player))
            })
            .collect()
    }
}
