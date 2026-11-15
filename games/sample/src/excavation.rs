//! Sample-owned terrain/tool balance, passed explicitly by application hosts.

pub const DEFAULT_TOOL_GRANTS: [Option<minimium_world::items::ItemRecord>; 2] = [
    Some(minimium_world::items::ItemRecord {
        kind: minimium_world::items::ToolKind::Shovel,
    }),
    Some(minimium_world::items::ItemRecord {
        kind: minimium_world::items::ToolKind::Pickaxe,
    }),
];

pub const DEFAULT_TOOL_PROFILE: minimium_simulation::tools::ToolProfile =
    minimium_simulation::tools::ToolProfile::new(
        [
            minimium_simulation::tools::ToolDefinition::new(
                minimium_world::items::ToolKind::Shovel,
                3.0,
                0.6,
                minimium_world::terrain::TerrainMaterialSet::SOIL,
            ),
            minimium_simulation::tools::ToolDefinition::new(
                minimium_world::items::ToolKind::Pickaxe,
                3.0,
                0.6,
                minimium_world::terrain::TerrainMaterialSet::SOIL_AND_ROCK,
            ),
        ],
        500,
    );

pub const DEFAULT_IMPACT_PROFILE: minimium_world::impact::ImpactProfile =
    minimium_world::impact::ImpactProfile {
        chance_per_10000: 100,
        initial_mass_kg: 500_000.0,
        density_kg_m3: 7800.0,
        crater_radius_m: 24.0,
    };
