//! A host that reads typed replies through the public crate API.
//! Compile with `cargo check -p minimium-server-runtime --example typed_replies`.
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
        "entity despawn 1 999",
        "stop",
    ] {
        runtime.submit(RuntimeCommand::parse(command)?)?;
    }
    for _ in 0..3 {
        time.monotonic += Duration::from_millis(50);
        let outcome = runtime.step(time)?;
        for result in outcome.commands {
            let reply = result.reply.view();
            println!(
                "command {}: {:?}, stopping={}",
                result.id.get(),
                reply.state,
                reply.should_stop
            );
            inspect_reply(reply);
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

fn inspect_reply(reply: CommandReplyView<'_>) {
    if reply.state == ReplyState::Rejected {
        println!("error {:?}: {:?}", reply.error_code, reply.detail);
    }
    if reply.state == ReplyState::Partial {
        if let Some(effects) = reply.effects {
            for effect in effects.results() {
                println!("effect {effect:?}");
            }
        }
    }
    if reply.should_stop {
        println!("runtime requested stop");
    }
    let Some(payload) = reply.payload else {
        return;
    };
    match payload {
        CommandPayloadView::Help {
            builtins,
            modules,
            truncated,
        } => {
            for command in builtins {
                println!("builtin {command}");
            }
            for module in modules {
                println!("{} {} args", module.usage(), module.arguments().len());
            }
            println!("help truncated={truncated}");
        }
        CommandPayloadView::Status(status) => {
            println!(
                "status players={} lanes={} ticks={}",
                status.players(),
                status.lanes(),
                status.ticks()
            );
            println!(
                "clock probes={}/{}/{} sync={} worlds={} startup={}",
                status.clock_probe_requests(),
                status.clock_probe_accepted(),
                status.clock_probe_rejected(),
                status.environment_sync_messages(),
                status.scheduled_world_instances(),
                status.startup_system_runs()
            );
            println!(
                "persistence pending={} last={:?}",
                status.persistence().pending_saves,
                status.persistence().last_save
            );
            if let Some(runtime) = status.runtime() {
                println!(
                    "runtime {} players={} tick={} snapshot={} autosave={} protocol={} save={} fingerprint={}",
                    runtime.game_address(),
                    runtime.max_players(),
                    runtime.tick_ms(),
                    runtime.snapshot_ms(),
                    runtime.autosave_seconds(),
                    runtime.protocol_version(),
                    runtime.save_version(),
                    runtime.map_fingerprint()
                );
                for profile in runtime.environment_profiles() {
                    println!("environment profile {profile:?}");
                }
                println!(
                    "environment profiles truncated={}",
                    runtime.environment_profiles_truncated()
                );
                let impact = runtime.impact();
                println!(
                    "impact active={} craters={} pending={} ready={}",
                    impact.active, impact.craters, impact.pending, impact.ready
                );
                let excavation = runtime.excavation();
                println!(
                    "excavation chunks={} pending={} bytes={} ready={} fluid cells/jobs={}/{} terrain/fluid bytes={}/{} reserved={} failed={}",
                    excavation.chunks(),
                    excavation.pending(),
                    excavation.bytes(),
                    excavation.ready(),
                    excavation.fluid_cells(),
                    excavation.fluid_jobs(),
                    excavation.terrain_payload_bytes(),
                    excavation.fluid_payload_bytes(),
                    excavation.fluid_reserved(),
                    excavation.failed_worlds()
                );
                for percentile in excavation.percentiles() {
                    println!("excavation percentile {percentile:?}");
                }
            }
        }
        CommandPayloadView::Players {
            rows,
            truncated,
            next_cursor,
        } => {
            for row in rows {
                println!(
                    "player {} {} world={} at {:?}",
                    row.id(),
                    row.name(),
                    row.world(),
                    row.position()
                );
            }
            println!("players truncated={truncated} next={next_cursor:?}");
        }
        CommandPayloadView::Metrics {
            snapshot,
            clients,
            clients_truncated,
        } => {
            for field in snapshot.fields() {
                let value = match field.value() {
                    MetricValue::Count(value) => value.to_string(),
                    MetricValue::Optional(value) => format!("{value:?}"),
                    MetricValue::Ratio(value) => value.to_string(),
                    MetricValue::Label(value) => (*value).to_owned(),
                };
                println!("metric {}={value}", field.name());
            }
            for row in clients {
                println!(
                    "traffic {} in={} out={} decoded={} queued={} encoding={} connected={}",
                    row.id(),
                    row.bytes_in(),
                    row.bytes_out(),
                    row.decoded_frames(),
                    row.queued_frames(),
                    row.encoding_us(),
                    row.connected_ms()
                );
            }
            println!("client traffic truncated={clients_truncated}");
        }
        CommandPayloadView::SaveQueued => {
            println!("save queued; status reports pending/completed state");
        }
        CommandPayloadView::Stopping => println!("stopping"),
        CommandPayloadView::Time(clock) => {
            println!(
                "clock id={} nonce={:?} mode={} rate={} offline={} utc={} revision={} fault={} correction={:?} dirty={:?}",
                clock.clock_id(),
                clock.session_nonce(),
                clock.mode(),
                clock.rate_ppm(),
                clock.offline_policy(),
                clock.utc_ms(),
                clock.revision(),
                clock.fault(),
                clock.correction_ms(),
                clock.dirty_revision()
            );
        }
        CommandPayloadView::Property(report) => {
            println!(
                "property {} requested={:?} effective={:?} default={:?} source={} reason={:?}",
                report.key(),
                report.value().requested,
                report.value().effective,
                report.value().default,
                report.value().source.as_str(),
                report.value().difference_reason
            );
        }
        CommandPayloadView::UnknownProperty(key) => println!("unknown property {key}"),
        CommandPayloadView::UnavailableProperty { key, reason } => {
            println!("property {key} unavailable: {reason}");
        }
        CommandPayloadView::PropertyHelp(spec) => {
            println!(
                "property {} {:?} default={:?} unit={:?} constraints={:?} owner={} permission={:?} lifecycle={:?} persistent={} resettable={} description={}",
                spec.key,
                spec.value_type,
                spec.default,
                spec.unit,
                spec.constraints,
                spec.owner,
                spec.permission,
                spec.lifecycle,
                spec.persistent,
                spec.resettable,
                spec.description
            );
        }
        CommandPayloadView::PropertyCommandsHelp { builtins } => {
            for command in builtins {
                println!("property command {command}");
            }
        }
        CommandPayloadView::SingleCommandHelp(help) => {
            println!(
                "command help {} args={}",
                help.usage(),
                help.arguments().len()
            );
        }
        CommandPayloadView::PropertyList {
            prefix,
            rows,
            truncated,
        } => {
            for row in rows {
                println!(
                    "property {} {:?} default={:?} unit={:?} constraints={:?} owner={} permission={:?} lifecycle={:?} persistent={} resettable={}",
                    row.key,
                    row.value_type,
                    row.default,
                    row.unit,
                    row.constraints,
                    row.owner,
                    row.permission,
                    row.lifecycle,
                    row.persistent,
                    row.resettable
                );
            }
            println!("property prefix={prefix:?} truncated={truncated}");
        }
        CommandPayloadView::WorldList {
            rows,
            truncated,
            next_cursor,
        } => {
            for row in rows {
                println!(
                    "world {} players={} topology={:?} generation={:?}",
                    row.id(),
                    row.players(),
                    row.topology(),
                    row.generation()
                );
            }
            println!("world list truncated={truncated} next={next_cursor:?}");
        }
        CommandPayloadView::WorldInspect(world) => {
            println!(
                "world {} players={} manifest={:?}",
                world.id(),
                world.players(),
                world.manifest()
            );
        }
        CommandPayloadView::EntityList {
            rows,
            truncated,
            next_cursor,
        } => {
            for row in rows {
                match row.kind() {
                    EntityKindView::Entity(id) => {
                        println!("entity world={} id={id}", row.world());
                    }
                    EntityKindView::Player(id, name) => {
                        println!("entity world={} player={id} name={name}", row.world());
                    }
                }
            }
            println!("entity list truncated={truncated} next={next_cursor:?}");
        }
        CommandPayloadView::EntityInspect {
            world,
            entity,
            rows,
            truncated,
            next_cursor,
        } => {
            println!("entity world={world} id={}", entity.get());
            for row in rows {
                println!(
                    "component {} schema={} bytes={}",
                    row.kind().get(),
                    row.schema().get(),
                    row.bytes()
                );
            }
            println!("entity components truncated={truncated} next={next_cursor:?}");
        }
        CommandPayloadView::EntityDespawn { world, entity } => {
            println!("despawn requested world={world} entity={}", entity.get());
        }
        CommandPayloadView::Bolid {
            id,
            kind,
            recipients,
            shower,
        } => {
            println!("bolid {id} kind={kind} recipients={recipients} shower={shower:?}");
        }
        CommandPayloadView::ImpactSpawn { id } => {
            println!("meteorid accepted {id}");
        }
        CommandPayloadView::ModuleText { text, truncated } => {
            println!("module text truncated={truncated}: {text}");
        }
    }
}
