//! Public bounded keyset pagination for embedded hosts.
//! Compile with `cargo check -p minimium-server-runtime --example paged_reads`.
#[path = "support/embedded_services.rs"]
mod services;
use minimium_server_runtime::{composition::*, *};
use minimium_simulation::tools::{ToolDefinition, ToolProfile};
use minimium_storage::{MemoryStorageBackend, MemoryStorageLimits, StorageBackend};
use minimium_world::world::*;
use minimium_world::{items::ToolKind, terrain::TerrainMaterialSet};
use std::{collections::BTreeMap, io, sync::Arc, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    let instance = WorldInstanceId::new(1).unwrap();
    let manifest = WorldManifest::new(
        WorldDefinitionId::new(1).unwrap(),
        WorldTopology::Dungeon,
        GenerationSpec::new(WorldSeed::new(0), GenerationRevision::new(6).unwrap(), 1).unwrap(),
        WorldContentRevision::new(1).unwrap(),
        GeneratorDigest::authored_map_fingerprint(map.fingerprint()),
    );
    let profile = minimium_procgen::TerrainMaterialProfile::new(500).unwrap();
    let worlds = vec![(instance, map.clone(), manifest)];
    let modules = RuntimeModules::new(&[])?;
    let storage = MemoryStorageBackend::new(MemoryStorageLimits {
        snapshot_bytes: 1024 * 1024,
        ..MemoryStorageLimits::default()
    })?;
    storage.preflight()?;
    let loaded = storage.load(&worlds, modules.components())?;
    let next_player = loaded.player_store.next_player_id()?;
    let durable = storage.open_services()?;
    let prepared = PreparedRuntime {
        worlds: Some(vec![WorldDefinition {
            instance,
            manifest,
            map: map.clone(),
            terrain_material_profile: profile.bind(manifest, map.fingerprint()),
            resolved_relief_policy: None,
            initial_entities: vec![],
        }]),
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
        game_address: "embedded".into(),
        endpoint: None,
        process_lifecycle: None,
        environment_config: Default::default(),
        prepared_geography: None,
    };
    let composition = RuntimeComposition {
        combat: ServerCombatConfig {
            hitscan: minimium_simulation::CombatSettings {
                damage: 10,
                range: 32.0,
            },
            fire_cooldown_ms: 500,
        },
        impacts: None,
        async_work_policy: AsyncWorkPolicy::default(),
        excavation: ExcavationConfig {
            terrain: minimium_terrain::TerrainConfig {
                max_chunks: 64,
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
                max_active_cells: 1024,
            },
            server_fluid_cells: 1024,
        },
        modules,
        loggers: services::loggers(),
        authentication: Arc::new(services::NoRemoteAuthentication),
        shutdown: ShutdownRequest::default(),
        // This host admits no remote session. A network host supplies a fresh random nonce.
        session_nonce: minimium_environment::time::SessionNonce::new([1; 16]),
    };
    let execution = RuntimeExecution::new(1, 1)?;
    let time = RuntimeTime::system_now();
    let runtime = ServerRuntime::new(
        RuntimeSettings {
            max_players: 8,
            autosave_seconds: 0,
            bolid_interval: None,
            resolved_settings: BTreeMap::new(),
        },
        prepared,
        composition,
        execution,
        time,
    )?;
    let limits = (32, 1024 * 1024);
    let mut request = CommandViewRequest::new(limits.0, limits.1)?;
    loop {
        match runtime.query(RuntimeQuery::WorldsPage(request.clone())) {
            RuntimeQueryResult::WorldsPage(Ok(page)) => {
                for world in page.rows {
                    println!("world {}", world.id.get());
                }
                let Some(cursor) = page.next else {
                    break;
                };
                if let RuntimeQueryResult::PlayersPage(Err(CommandViewError::StaleCursor)) = runtime
                    .query(RuntimeQuery::PlayersPage(
                        CommandViewRequest::new(limits.0, limits.1)?.after(cursor.clone()),
                    ))
                {
                    println!("cursor belongs to a different query");
                }
                request = CommandViewRequest::new(limits.0, limits.1)?.after(cursor);
            }
            RuntimeQueryResult::WorldsPage(Err(CommandViewError::InvalidLimit)) => {
                println!("invalid page limit")
            }
            RuntimeQueryResult::WorldsPage(Err(CommandViewError::StaleCursor)) => {
                println!("restart listing from the first page");
                break;
            }
            RuntimeQueryResult::WorldsPage(Err(CommandViewError::RecordTooLarge)) => {
                println!("one row exceeds the byte cap")
            }
            _ => break,
        }
    }
    let mut player_request = CommandViewRequest::new(limits.0, limits.1)?;
    loop {
        match runtime.query(RuntimeQuery::PlayersPage(player_request.clone())) {
            RuntimeQueryResult::PlayersPage(Ok(page)) => {
                for player in page.rows {
                    println!("player {} {}", player.id.get(), player.name);
                }
                let Some(cursor) = page.next else {
                    break;
                };
                player_request = CommandViewRequest::new(limits.0, limits.1)?.after(cursor);
            }
            RuntimeQueryResult::PlayersPage(Err(CommandViewError::StaleCursor)) => {
                println!("restart player listing");
                break;
            }
            RuntimeQueryResult::PlayersPage(Err(error)) => {
                println!("player page unavailable: {error}");
                break;
            }
            _ => break,
        }
    }
    let mut entity_request = CommandViewRequest::new(limits.0, limits.1)?;
    loop {
        match runtime.query(RuntimeQuery::EntityRows(entity_request.clone())) {
            RuntimeQueryResult::EntityRows(Ok(page)) => {
                println!("entity/player rows: {}", page.rows.len());
                let Some(cursor) = page.next else {
                    break;
                };
                entity_request = CommandViewRequest::new(limits.0, limits.1)?.after(cursor);
            }
            RuntimeQueryResult::EntityRows(Err(error)) => {
                println!("entity page unavailable: {error}");
                break;
            }
            _ => break,
        }
    }
    let components = CommandViewRequest::new(limits.0, limits.1)?;
    match runtime.query(RuntimeQuery::Components(
        instance,
        minimium_world::entity::EntityId::new(1).unwrap(),
        components,
    )) {
        RuntimeQueryResult::Components(Err(CommandViewError::UnknownEntity)) => {
            println!("entity has no component page")
        }
        RuntimeQueryResult::Components(Ok(page)) => {
            if let Some(cursor) = page.next {
                let continuation = CommandViewRequest::new(limits.0, limits.1)?.after(cursor);
                let _ = runtime.query(RuntimeQuery::Components(
                    instance,
                    minimium_world::entity::EntityId::new(1).unwrap(),
                    continuation,
                ));
            }
        }
        _ => {}
    }
    if let RuntimeQueryResult::World(Err(CommandViewError::UnknownWorld)) = runtime.query(
        RuntimeQuery::World(WorldInstanceId::new(u128::MAX).unwrap()),
    ) {
        println!("requested world no longer exists");
    }
    if let Err(CommandViewError::InvalidLimit) = CommandViewRequest::new(0, limits.1) {
        println!("zero-row requests are rejected");
    }
    let shutdown = runtime.shutdown(time, std::time::Instant::now() + Duration::from_secs(2));
    shutdown.result?;
    // Ports have been released; the same bounded memory backend can be reopened.
    let modules = RuntimeModules::new(&[])?;
    let restored = storage.load(&worlds, modules.components())?;
    println!("reopened snapshot version {:?}", restored.source_version);
    Ok(())
}
