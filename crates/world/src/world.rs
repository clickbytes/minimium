pub const CELL_SIZE: f32 = 32.0;
pub const PLAYER_RADIUS: f32 = 0.3;
pub const PLAYER_HEIGHT: f32 = 1.7;
pub const COLLISION_EPSILON: f32 = 0.0001;

mod addressing;
mod dungeon;
pub mod geometry;
mod gravity_policy;
mod map;
mod model;
mod peak_relief;
mod spatial;

pub use addressing::{
    CollisionCell, CubeSampleKey, DungeonCollisionCell, DungeonSectorKey, PlanetCollisionCell,
    PlanetCorner, PlanetEdge, PlanetFace, PlanetSectorKey, RenderPatchKey, SectorKey,
    PLANET_COLLISION_CELLS_PER_FACE, PLANET_DENSE_SECTORS_PER_FACE, PLANET_SAMPLE_COORDINATE_SCALE,
    PLANET_SECTORS_PER_FACE,
};
pub use dungeon::{DungeonBlueprint, DungeonRoom, DungeonTile};
pub use geometry::{triangle_area_squared, Aabb};
pub use gravity_policy::{character_frame, CharacterGravity, GravityPolicy};
pub use map::{
    MapBox, MapTriangle, WorldMap, MAX_MAP_BOXES, MAX_MAP_COORDINATE, MAX_MAP_TRIANGLES,
};
pub use model::{
    GenerationRevision, GenerationSpec, GeneratorDigest, MembershipEpoch, ObjectOriginId,
    PlanetDefinition, PlanetKind, Position3Mm, SectorRevision, TransitionId, WorldContentRevision,
    WorldDefinitionId, WorldInstanceId, WorldLocation, WorldManifest, WorldObjectState,
    WorldOrientationQ30, WorldSectorState, WorldSeed, WorldSource, WorldTopology,
    MAX_OBJECT_STATE_BYTES, MAX_PERSISTED_WORLD_OBJECT_STATES,
    MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES, MAX_PERSISTED_WORLD_SECTORS, MAX_SECTOR_OBJECT_STATES,
    MAX_SECTOR_OBJECT_STATE_BYTES,
};
pub use peak_relief::PeakRelief;
pub use spatial::{cell_for, Cell};
