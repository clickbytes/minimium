//! Mutable simulation state, movement, collision queries and combat.

pub mod impact;
mod simulation;
pub mod tools;

pub use simulation::SkyCoverQuery;
pub use simulation::{
    step_batch, step_character, step_character_mm, step_character_mm_with_water,
    step_character_with_water, CharacterState, CollisionIndex, CollisionQuery, CollisionQueryStats,
    CombatCoveragePlan, CombatRaySectorInterval, CombatSettings, MovementIntent, MovementPolicy,
    Player, ShotOutcome, ShotReadinessError, SimulationBatch, SpatialGeometryPublication,
    ToolRayCapsule, World, WorldSnapshot, MOVEMENT_LIMIT, PLAYER_MAX_HEALTH,
    SIMULATION_STEP_MILLISECONDS, SIMULATION_STEP_SECONDS,
};
