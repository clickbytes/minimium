use super::PlayerStore;
use minimium_world::world::WorldLocation;

impl PlayerStore {
    pub fn migrate_legacy_planet_positions(&mut self) {
        for shard in &mut self.players {
            for player in std::sync::Arc::make_mut(shard).values_mut() {
                if player.location.instance().get() == 1 {
                    player.location = remap_planet_location(player.location);
                    player.position = player
                        .location
                        .position()
                        .coordinates()
                        .map(|x| x as f32 / 1000.0);
                }
                if player.return_location.instance().get() == 1 {
                    player.return_location = remap_planet_location(player.return_location);
                }
            }
        }
    }
}

fn remap_planet_location(location: WorldLocation) -> WorldLocation {
    let old = location.position().coordinates().map(|x| x as f64);
    let length = old.iter().map(|x| x * x).sum::<f64>().sqrt();
    if length < 1.0 {
        return location;
    }
    let radius = 10_035_000.0 + (length - 100_000.0).max(0.0);
    let position = old.map(|x| (x / length * radius).round() as i64);
    WorldLocation::new(
        location.instance(),
        minimium_world::world::Position3Mm(position),
        location.orientation(),
    )
}
