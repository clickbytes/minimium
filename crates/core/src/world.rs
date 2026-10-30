const CELL_SIZE: f32 = 32.0;
const VISIBILITY_RADIUS: f32 = 72.0;
const PLAYER_RADIUS: f32 = 0.3;
const PLAYER_HEIGHT: f32 = 1.7;
const MAX_STEP_UP: f32 = 0.55;
const PLAYER_MOVE_SPEED: f32 = 5.0;
const PLAYER_JUMP_SPEED: f32 = 5.5;
pub const PLAYER_MAX_HEALTH: u16 = 100;
const MAX_PLAYER_NAME_CHARS: usize = 24;
const HITSCAN_DAMAGE: u16 = 34;
const HITSCAN_RANGE: f32 = 60.0;
const MAX_RAY_CELL_CROSSINGS: usize = 32;
const GRAVITY: f32 = 9.81;
const MAX_FALL_SPEED: f32 = 40.0;
const COLLISION_EPSILON: f32 = 0.0001;
const MAX_COLLIDER_CELLS: u64 = 64;
const MAX_QUERY_CELLS: u64 = 512;
const PARALLEL_SIMULATION_MIN_PLAYERS: usize = 32;
const PLAYERS_PER_SIMULATION_BATCH: usize = 16;

mod addressing;
mod collision_index;
mod collision_queries;
mod combat;
mod geometry;
mod gravity_policy;
mod map;
mod model;
mod precise_step;
mod raycast;
mod rules;
mod snapshot;
mod world_state;

pub use addressing::{
    CollisionCell, CubeSampleKey, DungeonCollisionCell, DungeonSectorKey, PlanetCollisionCell,
    PlanetCorner, PlanetEdge, PlanetFace, PlanetSectorKey, RenderPatchKey, SectorKey,
    PLANET_COLLISION_CELLS_PER_FACE, PLANET_DENSE_SECTORS_PER_FACE, PLANET_SAMPLE_COORDINATE_SCALE,
    PLANET_SECTORS_PER_FACE,
};
pub use collision_index::{Aabb, CollisionIndex};
pub use collision_queries::{CollisionQuery, CollisionQueryStats};
pub use geometry::triangle_area_squared;
pub use gravity_policy::GravityPolicy;
pub use map::{
    MapBox, MapTriangle, WorldMap, MAX_MAP_BOXES, MAX_MAP_COORDINATE, MAX_MAP_TRIANGLES,
};
pub use model::{
    GenerationRevision, GenerationSpec, GeneratorDigest, MembershipEpoch, ObjectOriginId,
    PlanetDefinition, PlanetKind, Position3Mm, SectorRevision, TransitionId, WorldContentRevision,
    WorldDefinitionId, WorldInstanceId, WorldLocation, WorldManifest, WorldObjectState,
    WorldOrientationQ30, WorldSectorState, WorldSeed, WorldTopology, MAX_OBJECT_STATE_BYTES,
    MAX_PERSISTED_WORLD_OBJECT_STATES, MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES,
    MAX_PERSISTED_WORLD_SECTORS, MAX_SECTOR_OBJECT_STATES, MAX_SECTOR_OBJECT_STATE_BYTES,
};
pub use precise_step::step_character_mm;
pub use rules::{
    character_frame, step_character, CharacterGravity, CharacterInput, CharacterState, Player,
    ShotOutcome,
};
pub use snapshot::WorldSnapshot;
pub use world_state::{step_batch, SimulationBatch, World};
