//! Static sample-game content shared by the client and server applications.

mod combat;
mod peak_relief;
mod planet_config;
mod portal;
pub use peak_relief::PeakReliefConfig;
mod scenario;
mod universe;

pub use combat::{SampleCombatConfig, DEFAULT_COMBAT_CONFIG};
pub use planet_config::{load as load_planet_preset, PlanetMode, ResolvedPlanetConfig};
pub use portal::{PortalComponent, SampleGameplayModule, PORTAL_COMPONENT, PORTAL_SCHEMA};
pub use scenario::{
    generate_demo_worlds, generate_worlds, generate_worlds_with_planet,
    generate_worlds_with_planet_and_geography, materialize_manifest_map, DemoPortal,
    GeneratedWorld,
};
pub use universe::{demo_universe_catalog, DemoUniverseCatalog};

#[cfg(test)]
mod tests;

/// Default seed used by the sample planet and connected dungeon scenario.
pub const DEFAULT_SEED: u128 = 0x0030_0000_0000_0001;

/// Presentation-only recipe, independent of generator/save revisions.
pub const SURFACE_RECIPE: minimium_procgen::surface_habitat::SurfaceRecipe =
    minimium_procgen::surface_habitat::SurfaceRecipe {
        landform_radius_m: 256.0,
        retention_strength: 0.65,
        exposure_strength: 0.45,
    };
