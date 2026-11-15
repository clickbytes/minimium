//! Deterministic, addressable generation primitives shared by client and server.

mod dungeon;
mod earth_observer;
mod error;
mod excavation_geometry;
mod generator_contract;
mod geography;
mod impact_geometry;
mod materialize;
mod philox;
mod planet;
mod planet_sector;
mod planet_teleport;
mod random_address;
mod sector_clip;
mod sector_geometry;
mod sector_neighborhood;
pub mod surface_habitat;
pub mod terrain_clip;
mod terrain_color;
mod terrain_contours;
mod terrain_material;
mod terrain_overlay;
mod terrain_seam;
pub mod terrain_surface;
mod traversal;
mod volume_base;
mod water_catalog;
pub use volume_base::PlanetVolumeBase;
pub use water_catalog::{
    WaterBody, WaterBodyDefinition, WaterCatalog, WaterCurvePoint, CONNECTED_OCEAN_ID,
    DEFAULT_WATER_CATALOG_RESOLUTION, MAX_WATER_CATALOG_RESOLUTION,
};

#[cfg(test)]
mod tests;

pub use dungeon::{generate_dungeon, materialize_dungeon_sector, DungeonSector, DungeonSpec};
pub use earth_observer::earth_height_reference;
pub use error::{GenerationError, GenerationErrorKind};
pub use excavation_geometry::{
    integrate_volume_region_with_contours, integrate_volume_sector,
    integrate_volume_sector_with_contours, validate_volume_geometry, volume_chunk_keys_near_region,
    volume_contour_sectors,
};
pub use generator_contract::{generator_digest, validate_world_manifest};
pub use geography::{EarthSampleError, PreparedGeography};
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
    planet_surface_for, PLANET_RADIUS_MM, PLANET_RENDER_BLOCK_SUBDIVISIONS,
};
pub use planet_teleport::{
    planet_collision_position_at_direction, planet_collision_position_at_direction_with_geography,
    planet_safe_spawn_position_at_direction_with_geography,
};
pub use random_address::{AddressedRandom, GenerationChannel};
pub use sector_geometry::{
    materialize_world_sector, materialize_world_sector_with_geography, slice_sector_geometry,
    SectorGeometry,
};
pub use sector_neighborhood::{
    planet_sector_neighborhood_with_diagonals, sector_neighborhood, sector_neighborhood_for,
    DUNGEON_SECTOR_EDGE,
};
pub use terrain_material::{ResolvedTerrainMaterialProfile, TerrainMaterialProfile};
pub use terrain_overlay::{SectorContentVersion, TerrainInputs};
pub use terrain_seam::ExcavationGeometryError;
pub use traversal::can_traverse_surface;
