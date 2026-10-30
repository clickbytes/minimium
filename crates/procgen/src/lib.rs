//! Deterministic, addressable generation primitives shared by client and server.

mod demo;
mod dungeon;
mod earth;
mod earth_observer;
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

pub use demo::{
    generate_demo_worlds, generate_legacy_demo_worlds, generate_worlds,
    generate_worlds_with_planet, generate_worlds_with_planet_revision, generator_digest,
    DemoPortal, GeneratedWorld,
};
pub use dungeon::{
    generate_dungeon, DungeonBlueprint, DungeonRoom, DungeonSector, DungeonSpec, DungeonTile,
};
pub use earth::{
    earth_elevation_mm, earth_is_land, ensure_earth_from_manifest, install_earth_package,
    try_earth_elevation_m, try_earth_elevation_m_for_package, EarthSampleError,
};
pub use earth_observer::earth_height_reference;
use materialize::materialize_legacy_planet_map;
pub use materialize::{
    materialize_dungeon_map, materialize_planet_map, materialize_planet_overview,
};
pub use philox::philox4x32_10;
pub use planet::{PlanetLandmarks, PlanetPoint, PlanetSurfaceSpec, PlanetTerrain};
pub use planet_sector::{
    generate_planet_render_block, generate_planet_render_patch, generate_planet_sector,
    planet_point_at_direction, planet_sector_count, planet_surface, planet_surface_for,
    PLANET_RADIUS_MM,
};
pub use planet_teleport::planet_collision_position_at_direction;
pub use random_address::{AddressedRandom, GenerationChannel};
pub use sector_geometry::{materialize_world_sector, slice_sector_geometry, SectorGeometry};
pub use sector_neighborhood::{sector_neighborhood, sector_neighborhood_for, DUNGEON_SECTOR_EDGE};
pub use traversal::can_traverse_surface;
