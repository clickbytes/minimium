//! Sample-owned terrain/tool balance, passed explicitly by application hosts.

pub const DEFAULT_TOOL_PROFILE: minimium_simulation::tools::ToolProfile =
    minimium_simulation::tools::ToolProfile {
        reach_m: 3.0,
        radius_m: 0.6,
        cooldown_ms: 500,
    };

pub const DEFAULT_IMPACT_PROFILE: minimium_world::impact::ImpactProfile =
    minimium_world::impact::ImpactProfile {
        chance_per_10000: 100,
        initial_mass_kg: 500_000.0,
        density_kg_m3: 7800.0,
        crater_radius_m: 24.0,
    };
