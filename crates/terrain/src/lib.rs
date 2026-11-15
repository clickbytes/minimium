//! Pure volumetric terrain. Hosts own scheduling, persistence and publication.
mod craters;
mod edit;
pub use craters::TerrainSnapshot;
mod field;
mod mesh;
mod query;

pub use edit::{
    PreparedTerrainEdit, TerrainDeformation, TerrainDeformationError, TerrainEditError,
    TerrainEditRequest,
};
pub use field::{BaseTerrain, ChunkImage, TerrainConfig, TerrainStore, TerrainView};
pub use mesh::{TerrainMesh, TerrainVertex};
