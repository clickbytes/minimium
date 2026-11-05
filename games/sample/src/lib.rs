//! Static sample-game content shared by the client and server applications.

mod combat;
mod planet_config;
mod scenario;
mod universe;

pub use combat::{SampleCombatConfig, DEFAULT_COMBAT_CONFIG};
pub use planet_config::{load as load_planet_preset, PlanetMode, ResolvedPlanetConfig};
pub use scenario::{
    generate_demo_worlds, generate_legacy_demo_worlds, generate_worlds,
    generate_worlds_with_planet, generate_worlds_with_planet_revision,
    generate_worlds_with_planet_revision_and_geography, DemoPortal, GeneratedWorld,
};
pub use universe::{demo_universe_catalog, DemoUniverseCatalog};

#[cfg(test)]
mod tests;

/// Default seed used by the sample planet and connected dungeon scenario.
pub const DEFAULT_SEED: u128 = 0x0030_0000_0000_0001;

/// Legacy map retained for existing demo worlds and migration compatibility.
pub const SAMPLE_MAP: &str = include_str!("../maps/demo.map");
