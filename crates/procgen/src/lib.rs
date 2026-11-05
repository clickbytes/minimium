//! Deterministic, addressable generation primitives shared by client and server.

mod dungeon;
mod earth_observer;
mod generator_contract;
mod geography;
mod materialize;
mod philox;
mod planet;
mod planet_sector;
mod planet_teleport;
mod random_address;
mod sector_geometry;
mod sector_neighborhood;
mod terrain_color;
mod traversal;

#[cfg(test)]
mod tests;

pub use dungeon::{generate_dungeon, materialize_dungeon_sector, DungeonSector, DungeonSpec};
pub use earth_observer::earth_height_reference;
pub use generator_contract::{generator_digest, validate_world_manifest};
pub use geography::{EarthSampleError, PreparedGeography};
pub use materialize::materialize_legacy_planet_map;
pub use materialize::{
    materialize_dungeon_map, materialize_planet_map, materialize_planet_overview,
};
pub use minimium_world::world::{DungeonBlueprint, DungeonRoom, DungeonTile};
pub use philox::philox4x32_10;
pub use planet::{PlanetLandmarks, PlanetPoint, PlanetSurfaceSpec, PlanetTerrain};
pub use planet_sector::{
    generate_planet_render_block, generate_planet_render_block_with_geography,
    generate_planet_render_patch, generate_planet_render_patch_with_geography,
    generate_planet_sector, generate_planet_sector_with_geography, planet_point_at_direction,
    planet_point_at_direction_with_geography, planet_sector_count, planet_surface,
    planet_surface_for, PLANET_RADIUS_MM,
};
pub use planet_teleport::{
    planet_collision_position_at_direction, planet_collision_position_at_direction_with_geography,
};
pub use random_address::{AddressedRandom, GenerationChannel};
pub use sector_geometry::{
    materialize_world_sector, materialize_world_sector_with_geography, slice_sector_geometry,
    SectorGeometry,
};
pub use sector_neighborhood::{sector_neighborhood, sector_neighborhood_for, DUNGEON_SECTOR_EDGE};
pub use traversal::can_traverse_surface;
