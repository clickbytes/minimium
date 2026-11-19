//! A host that reads typed replies through the public crate API.
//! Compile with `cargo check -p minimium-server-runtime --example typed_replies`.
#[path = "support/typed_reply_output.rs"]
mod reply_output;
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
    let mut time = RuntimeTime::system_now();
    let mut runtime = ServerRuntime::new(
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
    for command in [
        "world list",
        "world inspect 1",
        "get server.autosave_interval",
        "world save",
        "server.persistence.status()",
        "world.time.diagnostics()",
        "entity despawn 1 999",
        "stop",
    ] {
        runtime.submit(RuntimeCommand::parse(command)?)?;
    }
    for _ in 0..3 {
        time.monotonic += Duration::from_millis(50);
        let outcome = runtime.step(time)?;
        for result in outcome.commands {
            let operation = result.operation_result();
            println!(
                "{} request={} stage={} stop={}",
                operation.identity.operation.0,
                operation.identity.request_id,
                operation.receipt.stage.label(),
                operation.receipt.should_stop
            );
            let reply = match operation.outcome {
                Ok(reply) => reply,
                Err(error) => {
                    println!("{} {}", error.code, error.reason);
                    continue;
                }
            };
            println!(
                "command {}: {:?}, stopping={}",
                result.id.get(),
                reply.state,
                reply.should_stop
            );
            reply_output::inspect_reply(reply);
        }
        if outcome.stopping {
            break;
        }
    }
    if let RuntimeQueryResult::Status(status) = runtime.query(RuntimeQuery::Status) {
        println!(
            "submission {}: tick {}, worlds {}, persistence {:?}",
            "typed replies",
            status.simulation_tick,
            status.world_count,
            status.persistence.last_save
        );
    }
    let shutdown = runtime.shutdown(time, std::time::Instant::now() + Duration::from_secs(2));
    shutdown.result?;
    // Ports have been released; the same bounded memory backend can be reopened.
    let modules = RuntimeModules::new(&[])?;
    let restored = storage.load(&worlds, modules.components())?;
    println!("reopened snapshot version {:?}", restored.source_version);
    Ok(())
}
