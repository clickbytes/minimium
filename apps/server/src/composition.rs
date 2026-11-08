use minimium_game_sample::{
    generate_worlds, generate_worlds_with_planet_and_geography, load_planet_preset, GeneratedWorld,
    PlanetMode, ResolvedPlanetConfig, DEFAULT_COMBAT_CONFIG,
};
use minimium_procgen::PreparedGeography;
use minimium_server_runtime::composition::{
    InitialComponent, InitialEntity, ServerCombatConfig, ServerComposition, ShutdownRequest,
    WorldComposition, WorldCompositionRequest, WorldDefinition,
};
use minimium_world::world::WorldInstanceId;
use std::{
    error::Error,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

pub(super) fn server_composition(
    dev_mode: bool,
    earth_package_dir: Option<PathBuf>,
    server_logger: minimium_diagnostics::DomainLogger,
    storage_logger: minimium_diagnostics::DomainLogger,
    transport_logger: minimium_diagnostics::DomainLogger,
    game_sample_logger: minimium_diagnostics::DomainLogger,
    environment_logger: minimium_diagnostics::DomainLogger,
    protocol_logger: minimium_diagnostics::DomainLogger,
    world_logger: minimium_diagnostics::DomainLogger,
    sectors_logger: minimium_diagnostics::DomainLogger,
    simulation_logger: minimium_diagnostics::DomainLogger,
    asset_io_logger: minimium_diagnostics::DomainLogger,
    procgen_logger: minimium_diagnostics::DomainLogger,
    shutdown_request: ShutdownRequest,
) -> ServerComposition {
    let world_composer_logger = game_sample_logger.clone();
    let world_asset_logger = asset_io_logger.clone();
    let world_procgen_logger = procgen_logger.clone();
    ServerComposition {
        shutdown_request,
        modules: vec![Arc::new(minimium_game_sample::SampleGameplayModule)],
        world_composer: Arc::new(move |request| {
            match compose_world(
                request,
                earth_package_dir.as_deref(),
                &world_asset_logger,
                &world_procgen_logger,
            ) {
                Ok(composition) => Ok(composition),
                Err(error) => {
                    if let Ok(code) =
                        minimium_diagnostics::EventCode::new("game_sample.world_compose_failed")
                    {
                        world_composer_logger.emit(
                            world_composer_logger
                                .event(minimium_diagnostics::Level::Error, code)
                                .field("reason", &error.to_string()),
                        );
                    }
                    Err(error)
                }
            }
        }),
        combat: ServerCombatConfig {
            hitscan: DEFAULT_COMBAT_CONFIG.hitscan,
            fire_cooldown_ms: DEFAULT_COMBAT_CONFIG.fire_cooldown_ms,
        },
        server_logger,
        storage_logger,
        transport_logger,
        environment_logger,
        protocol_logger,
        world_logger,
        sectors_logger,
        simulation_logger,
        asset_io_logger,
        authentication_provider: Arc::new(
            super::developer_auth::DeveloperAuthenticationProvider::new(dev_mode),
        ),
    }
}

fn compose_world(
    request: WorldCompositionRequest,
    earth_package_dir: Option<&Path>,
    asset_io_logger: &minimium_diagnostics::DomainLogger,
    procgen_logger: &minimium_diagnostics::DomainLogger,
) -> Result<WorldComposition, Box<dyn Error>> {
    let WorldCompositionRequest {
        data_dir,
        requested_seed,
        existing_seed,
        map_path,
        world_definition_path,
        planet_config_path,
    } = request;
    let resolved_path = data_dir.join("world.planet.ron");
    let saved_planet = match std::fs::read_to_string(&resolved_path) {
        Ok(source) => Some(
            ron::from_str::<ResolvedPlanetConfig>(&source).map_err(|error| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("saved planet definition: {error}"),
                )
            })?,
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let (Some(file_seed), Some(planet)) = (existing_seed, &saved_planet) {
        if file_seed != planet.seed {
            return Err("world.seed differs from the saved planet definition".into());
        }
    }
    if let (Some(expected), Some(requested)) = (
        existing_seed.or(saved_planet.as_ref().map(|planet| planet.seed)),
        requested_seed,
    ) {
        if expected != requested {
            return Err("--seed differs from the seed stored in this data directory".into());
        }
    }
    let seed = match existing_seed
        .or(saved_planet.as_ref().map(|planet| planet.seed))
        .or(requested_seed)
    {
        Some(seed) => seed,
        None => {
            let token = minimium_platform::endpoint::new_admin_token()?;
            u128::from_str_radix(&token[..32], 16)
                .expect("an admin token contains hexadecimal digits")
        }
    };
    let mut planet = if map_path.is_none() && world_definition_path.is_none() {
        let requested = planet_config_path
            .as_deref()
            .map(|path| load_planet_preset(path, seed))
            .transpose()?;
        let resolved = match (saved_planet, requested) {
            (Some(saved), Some(mut requested)) => {
                requested.package_digest = saved.package_digest;
                if saved != requested {
                    return Err("planet preset differs from the immutable definition in this data directory".into());
                }
                saved
            }
            (Some(saved), _) => saved,
            (None, Some(requested)) => requested,
            (None, None) => ResolvedPlanetConfig::procedural(seed),
        };
        resolved.validate()?;
        Some(resolved)
    } else {
        None
    };

    let mut prepared_geography = None;
    let (primary_map, worlds) = if let Some(path) = world_definition_path.as_deref() {
        let definition = minimium_asset_io::load_world_definition(path).map_err(|error| {
            emit_failure(asset_io_logger, error.code(), &error.to_string());
            error
        })?;
        minimium_procgen::validate_world_manifest(definition.manifest()).map_err(|error| {
            emit_failure(procgen_logger, error.code(), &error.to_string());
            io::Error::new(io::ErrorKind::InvalidData, error)
        })?;
        let map =
            minimium_procgen::materialize_dungeon_map(definition.dungeon()).map_err(|error| {
                emit_failure(procgen_logger, error.code(), &error.to_string());
                io::Error::new(io::ErrorKind::InvalidData, error)
            })?;
        if definition.manifest().generator_digest()
            != minimium_world::world::GeneratorDigest::authored_map_fingerprint(map.fingerprint())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "authored world definition digest does not match its materialized map",
            )
            .into());
        }
        let instance = WorldInstanceId::new(4).expect("manual world instance ID is nonzero");
        let generated = WorldDefinition {
            instance,
            manifest: definition.manifest(),
            map: map.clone(),
            initial_entities: Vec::new(),
        };
        (map, Some(vec![generated]))
    } else if let Some(path) = map_path.as_deref() {
        let map = minimium_asset_io::load_map(path).map_err(|error| {
            emit_failure(asset_io_logger, error.code(), &error.to_string());
            error
        })?;
        (map, None)
    } else {
        let generated = if let Some(planet) = &mut planet {
            if planet.mode == PlanetMode::Earth {
                let package = planet
                    .earth_package
                    .clone()
                    .or_else(|| earth_package_dir.map(Path::to_path_buf))
                    .unwrap_or_else(|| data_dir.join("gebco-2026"));
                if !package.join("index.ron").is_file() {
                    return Err(format!(
                        "GEBCO 2026 ice-surface package missing at {}; run import-gebco.ps1 -GridPath GEBCO_2026.nc -DataDir {}",
                        package.display(), data_dir.display()
                    ).into());
                }
                let prepared = minimium_asset_io::open_earth_package(
                    &package,
                    (planet.package_digest != [0; 32]).then_some(planet.package_digest),
                )
                .map_err(|error| {
                    emit_failure(asset_io_logger, error.code(), &error.to_string());
                    io::Error::new(io::ErrorKind::InvalidData, error)
                })?;
                let digest = prepared.package_digest();
                if planet.package_digest != [0; 32] && planet.package_digest != digest {
                    return Err("GEBCO package differs from the immutable world definition".into());
                }
                planet.package_digest = digest;
                prepared_geography = Some(prepared.clone());
            }
            generate_worlds_with_planet_and_geography(
                seed,
                planet.definition(),
                prepared_geography.as_ref().map(|geography| {
                    geography.clone() as Arc<dyn minimium_procgen::PreparedGeography>
                }),
            )
        } else {
            generate_worlds(seed)
        };
        let worlds = generated
            .map_err(|error| {
                emit_failure(procgen_logger, error.code(), &error.to_string());
                io::Error::new(io::ErrorKind::InvalidData, error)
            })?
            .into_iter()
            .map(to_world_definition)
            .collect::<Vec<_>>();
        (worlds[0].map.clone(), Some(worlds))
    };
    let planet_definition = planet
        .map(|planet| ron::to_string(&planet).map(String::into_bytes))
        .transpose()?;

    Ok(WorldComposition {
        seed,
        primary_map,
        worlds,
        planet_definition,
        prepared_geography: prepared_geography
            .map(|geography| geography as Arc<dyn minimium_procgen::PreparedGeography>),
    })
}

fn emit_failure(logger: &minimium_diagnostics::DomainLogger, code: &'static str, reason: &str) {
    if let Ok(code) = minimium_diagnostics::EventCode::new(code) {
        logger.emit(
            logger
                .event(minimium_diagnostics::Level::Error, code)
                .field("reason", reason),
        );
    }
}

fn to_world_definition(world: GeneratedWorld) -> WorldDefinition {
    let initial_entities = world
        .portal
        .map(|portal| {
            let component = minimium_game_sample::PortalComponent {
                position: portal.position,
                target_world: portal.target_instance,
                destination: portal.destination,
            };
            let bytes = component
                .encode()
                .expect("generated portal coordinates are finite");
            let sector = minimium_procgen::sector_neighborhood_for(world.manifest, portal.position)
                .into_iter()
                .next()
                .expect("a portal belongs to a materialized world sector");
            InitialEntity {
                sector,
                components: vec![InitialComponent {
                    kind: minimium_game_sample::PORTAL_COMPONENT,
                    schema: minimium_game_sample::PORTAL_SCHEMA,
                    bytes,
                }],
            }
        })
        .into_iter()
        .collect();
    WorldDefinition {
        instance: world.instance,
        manifest: world.manifest,
        map: world.map,
        initial_entities,
    }
}
