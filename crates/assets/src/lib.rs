pub mod ktx2;
pub mod land_cover;
mod map;
mod obj;
mod sky_catalog;
pub mod terrain_material;
#[cfg(test)]
mod tests;
mod world_definition;
pub use map::{
    parse_map, parse_map_with_mesh_loader, MAX_MAP_BYTES, MAX_MAP_TOTAL_MESH_BYTES, MAX_MESH_BYTES,
};
pub use obj::{parse_obj_triangles, ObjMeshOptions};
pub use sky_catalog::{
    embedded_sky_catalog, embedded_sky_catalog_identity, embedded_star_labels, CatalogError,
    InvalidRecordReason, SkyCatalogIdentity, StarCatalogue, StarRecord,
};
pub use world_definition::{parse_world_definition, WorldDefinition, MAX_DEFINITION_BYTES};
