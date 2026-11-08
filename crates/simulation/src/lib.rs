//! Mutable simulation state, movement, collision queries and combat.

mod simulation;

pub use simulation::SkyCoverQuery;
pub use simulation::{
    step_batch, step_character, step_character_mm, CharacterState, CollisionIndex, CollisionQuery,
    CollisionQueryStats, CombatSettings, MovementIntent, MovementPolicy, Player, ShotOutcome,
    SimulationBatch, World, WorldSnapshot, MOVEMENT_LIMIT, PLAYER_MAX_HEALTH,
    SIMULATION_STEP_MILLISECONDS, SIMULATION_STEP_SECONDS,
};
