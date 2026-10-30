//! Deterministic, addressable generation primitives shared by client and server.

mod demo;
mod dungeon;
mod materialize;
mod philox;
mod planet;
mod random_address;
mod sector_geometry;
mod sector_neighborhood;

#[cfg(test)]
mod tests;

pub use demo::{generate_demo_worlds, generator_digest, DemoPortal, GeneratedWorld};
pub use dungeon::{
    generate_dungeon, DungeonBlueprint, DungeonRoom, DungeonSector, DungeonSpec, DungeonTile,
};
pub use materialize::{materialize_dungeon_map, materialize_planet_map};
pub use philox::philox4x32_10;
pub use planet::{PlanetLandmarks, PlanetPoint, PlanetSurfaceSpec, PlanetTerrain};
pub use random_address::{AddressedRandom, GenerationChannel};
pub use sector_geometry::{slice_sector_geometry, SectorGeometry};
pub use sector_neighborhood::{sector_neighborhood, DUNGEON_SECTOR_EDGE};
