#[path = "embedded_services.rs"]
mod services;

use minimium_server_runtime::{composition::*, *};
use minimium_simulation::tools::{ToolDefinition, ToolProfile};
use minimium_storage::{MemoryStorageBackend, MemoryStorageLimits, StorageBackend};
use minimium_world::world::*;
use minimium_world::{items::ToolKind, terrain::TerrainMaterialSet};
use std::{collections::BTreeMap, io, sync::Arc};

pub fn new_runtime(
    modules: &[Arc<ServerGameplayModule>],
) -> io::Result<(ServerRuntime, [WorldInstanceId; 2], RuntimeTime)> {
    let map = WorldMap::from_parts(
        [0.0, 1.0, 0.0],
        vec![MapBox {
            center: [0.0, -0.5, 0.0],
            size: [32.0, 1.0, 32.0],
            color: [0.5; 3],
            solid: true,
        }],
        vec![],
    )
    .map_err(io::Error::other)?;
    let instances = [
        WorldInstanceId::new(1).expect("world IDs are nonzero"),
        WorldInstanceId::new(2).expect("world IDs are nonzero"),
    ];
    let manifest = WorldManifest::new(
        WorldDefinitionId::new(1).expect("definition IDs are nonzero"),
        WorldTopology::Dungeon,
        GenerationSpec::new(
            WorldSeed::new(0),
            GenerationRevision::new(6).expect("generation revisions are nonzero"),
            1,
        )
        .expect("the example generation spec is valid"),
        WorldContentRevision::new(1).expect("content revisions are nonzero"),
        GeneratorDigest::authored_map_fingerprint(map.fingerprint()),
    );
    let profile = minimium_procgen::TerrainMaterialProfile::new(500)
        .expect("the example soil depth is in range");
    let storage_worlds: Vec<_> = instances
        .into_iter()
        .map(|instance| (instance, map.clone(), manifest))
        .collect();
    let modules = RuntimeModules::new(modules)?;
    let storage = MemoryStorageBackend::new(MemoryStorageLimits {
        snapshot_bytes: 1024 * 1024,
        ..MemoryStorageLimits::default()
    })?;
    storage.preflight()?;
    let loaded = storage.load(&storage_worlds, modules.components())?;
    let next_player = loaded.player_store.next_player_id()?;
    let durable = storage.open_services()?;
    let definitions = instances
        .into_iter()
        .map(|instance| WorldDefinition {
            instance,
            manifest,
            map: map.clone(),
            terrain_material_profile: profile.bind(manifest, map.fingerprint()),
            resolved_relief_policy: None,
            initial_entities: vec![],
        })
        .collect();
    let prepared = PreparedRuntime {
        worlds: Some(definitions),
        map,
        primary_terrain_material_profile: profile,
        player_store: loaded.player_store,
        entity_counters: loaded.entity_counters,
        world_initialization: loaded.world_initialization,
        entities: loaded.entities,
        saved_clock: loaded.saved_clock,
        next_player,
        persistence: durable.persistence,
        excavation_journal: durable.excavation,
        excavation_restore: durable.excavation_restore,
        impact_journal: durable.impacts,
        impact_restore: durable.impact_restore,
        climate: Default::default(),
        content: Box::new(services::NoContentEndpoint),
        transport: Box::new(services::NoNetwork),
        game_address: "owned-components-example".into(),
        endpoint: None,
        process_lifecycle: None,
        environment_config: Default::default(),
        prepared_geography: None,
    };
    let composition = RuntimeComposition {
        combat: ServerCombatConfig {
            hitscan: minimium_simulation::CombatSettings {
                damage: 1,
                range: 16.0,
            },
            fire_cooldown_ms: 500,
        },
        impacts: None,
        async_work_policy: AsyncWorkPolicy::default(),
        excavation: ExcavationConfig {
            terrain: minimium_terrain::TerrainConfig {
                max_chunks: 8,
                max_edit_radius_m: 0.6,
            },
            tools: ToolProfile::new(
                [
                    ToolDefinition::new(ToolKind::Shovel, 1.0, 0.25, TerrainMaterialSet::SOIL),
                    ToolDefinition::new(ToolKind::Pickaxe, 1.0, 0.25, TerrainMaterialSet::ROCK),
                ],
                500,
            ),
            tool_grants: [None; 2],
            fluids: minimium_fluids::FluidConfig {
                max_active_cells: 128,
            },
            server_fluid_cells: 128,
        },
        modules,
        loggers: services::loggers(),
        authentication: Arc::new(services::NoRemoteAuthentication),
        shutdown: ShutdownRequest::default(),
        session_nonce: minimium_environment::time::SessionNonce::new([1; 16]),
    };
    let runtime = ServerRuntime::new(
        RuntimeSettings {
            max_players: 1,
            autosave_seconds: 0,
            bolid_interval: None,
            resolved_settings: BTreeMap::new(),
        },
        prepared,
        composition,
        RuntimeExecution::new(1, 1)?,
        RuntimeTime::system_now(),
    )?;
    Ok((runtime, instances, RuntimeTime::system_now()))
}
