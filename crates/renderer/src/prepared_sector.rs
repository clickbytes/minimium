//! CPU-only preparation suitable for a host worker; no device or queue access.
use crate::{
    geometry::{self, Vertex},
    shadow_geometry::ShadowBounds,
    terrain_geometry::TerrainVertex,
};
use minimium_procgen::{PlanetTerrain, SectorGeometry};
use minimium_world::world::SectorKey;
use std::sync::Arc;
pub struct PreparedSector {
    pub key: SectorKey,
    pub lod: u8,
    pub geometry: Arc<SectorGeometry>,
    pub surface: Vec<TerrainVertex>,
    pub ocean: Vec<crate::ocean_geometry::OceanVertex>,
    pub objects: Vec<Vertex>,
    pub shadow: Vec<Vertex>,
    pub bounds: Option<ShadowBounds>,
    pub terrain: bool,
}
impl PreparedSector {
    pub fn new(
        key: SectorKey,
        lod: u8,
        geometry: Arc<SectorGeometry>,
        terrain: Option<&PlanetTerrain>,
        ocean: Option<&SectorGeometry>,
        origin: [f32; 3],
    ) -> Self {
        let exposed = ocean.map(|s| crate::ocean_geometry::exposed_ground(&geometry, s));
        let visible = exposed.as_ref().unwrap_or(&geometry);
        let vertices = geometry::sector_geometry_relative(visible, origin);
        let objects = if terrain.is_some() {
            vertices[..geometry.boxes.len() * 36].to_vec()
        } else {
            Vec::new()
        };
        let surface = terrain.map_or_else(Vec::new, |t| {
            crate::terrain_geometry::vertices(visible, t, origin)
        });
        let shadow = if terrain.is_some() {
            crate::terrain_geometry::shadow_vertices(visible, vertices)
        } else {
            vertices
        };
        let bounds = crate::shadow_geometry::geometry_bounds(visible)
            .into_iter()
            .chain(ocean.and_then(crate::shadow_geometry::geometry_bounds))
            .reduce(|a, b| ShadowBounds {
                minimum: std::array::from_fn(|i| a.minimum[i].min(b.minimum[i])),
                maximum: std::array::from_fn(|i| a.maximum[i].max(b.maximum[i])),
            });
        Self {
            key,
            lod,
            geometry,
            surface,
            ocean: ocean.map_or_else(Vec::new, |g| crate::ocean_geometry::vertices(g, origin)),
            objects,
            shadow,
            bounds,
            terrain: terrain.is_some(),
        }
    }
}
