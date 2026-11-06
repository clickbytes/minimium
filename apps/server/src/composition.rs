use minimium_game_sample::{
    generate_legacy_demo_worlds, generate_worlds,
    generate_worlds_with_planet_revision_and_geography, load_planet_preset, GeneratedWorld,
    PlanetMode, ResolvedPlanetConfig, DEFAULT_COMBAT_CONFIG, DEFAULT_SEED, SAMPLE_MAP,
};
use minimium_procgen::PreparedGeography;
use minimium_server_runtime::composition::{
    ServerCombatConfig, ServerComposition, WorldComposition, WorldCompositionRequest,
    WorldDefinition, WorldPortal,
};
use minimium_world::world::{
    GenerationRevision, GenerationSpec, GeneratorDigest, WorldContentRevision, WorldDefinitionId,
    WorldInstanceId, WorldManifest, WorldMap, WorldSeed, WorldTopology,
};
use std::{env, error::Error, io, path::PathBuf, sync::Arc};

pub(super) fn server_composition() -> ServerComposition {
    ServerComposition {
        world_composer: compose_world,
        combat: ServerCombatConfig {
            hitscan: DEFAULT_COMBAT_CONFIG.hitscan,
            fire_cooldown_ms: DEFAULT_COMBAT_CONFIG.fire_cooldown_ms,
        },
    }
}

fn compose_world(request: WorldCompositionRequest) -> Result<WorldComposition, Box<dyn Error>> {
    let WorldCompositionRequest {
        data_dir,
        requested_seed,
        existing_seed,
        snapshot_seed,
        map_path,
        world_definition_path,
        planet_config_path,
        legacy_snapshot,
        random_seed,
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
        existing_seed
            .or(snapshot_seed)
            .or(saved_planet.as_ref().map(|planet| planet.seed)),
        requested_seed,
    ) {
        if expected != requested {
            return Err("--seed differs from the seed stored in this data directory".into());
        }
    }
    if legacy_snapshot && requested_seed.is_some_and(|requested| requested != DEFAULT_SEED) {
        return Err("existing demo snapshot requires its original seed during migration".into());
    }
    let seed = existing_seed
        .or(snapshot_seed)
        .or(saved_planet.as_ref().map(|planet| planet.seed))
        .or(requested_seed)
        .or(legacy_snapshot.then_some(DEFAULT_SEED))
        .unwrap_or(random_seed);
    let legacy_generated = (existing_seed.is_some() || data_dir.join("world.snapshot").exists())
        && saved_planet.is_none();
    if legacy_generated && planet_config_path.is_some() {
        return Err("existing generated world has a legacy definition; use a new data directory for a planet preset".into());
    }
    let mut planet = if map_path.is_none() && world_definition_path.is_none() && !legacy_generated {
        let requested = planet_config_path
            .as_deref()
            .map(|path| load_planet_preset(path, seed))
            .transpose()?;
        let resolved = match (saved_planet, requested) {
            (Some(saved), Some(mut requested)) => {
                requested.package_digest = saved.package_digest;
                requested.generator_revision = saved.generator_revision;
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
            io::Error::new(
                error.kind(),
                format!(
                    "could not load world definition {}: {error}",
                    path.display()
                ),
            )
        })?;
        minimium_procgen::validate_world_manifest(definition.manifest())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let map = minimium_procgen::materialize_dungeon_map(definition.dungeon())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        let instance = WorldInstanceId::new(4).expect("manual world instance ID is nonzero");
        let generated = WorldDefinition {
            instance,
            manifest: definition.manifest(),
            map: map.clone(),
            portal: None,
        };
        let legacy_map = parse_legacy_map()?;
        (map, Some(vec![generated, legacy_world(legacy_map)]))
    } else if let Some(path) = map_path.as_deref() {
        let map = minimium_asset_io::load_map(path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("could not load map {}: {error}", path.display()),
            )
        })?;
        (map, None)
    } else {
        let generated = if let Some(planet) = &mut planet {
            if planet.mode == PlanetMode::Earth {
                let package = planet
                    .earth_package
                    .clone()
                    .or_else(|| env::var_os("MINIMIUM_EARTH_PACKAGE").map(PathBuf::from))
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
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                let digest = prepared.package_digest();
                if planet.package_digest != [0; 32] && planet.package_digest != digest {
                    return Err("GEBCO package differs from the immutable world definition".into());
                }
                planet.package_digest = digest;
                prepared_geography = Some(prepared.clone());
            }
            generate_worlds_with_planet_revision_and_geography(
                seed,
                planet.definition(),
                planet.generator_revision,
                prepared_geography.as_ref().map(|geography| {
                    geography.clone() as Arc<dyn minimium_procgen::PreparedGeography>
                }),
            )
        } else {
            generate_worlds(seed)
        };
        let mut worlds = generated
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?
            .into_iter()
            .map(to_world_definition)
            .collect::<Vec<_>>();
        worlds.push(legacy_world(parse_legacy_map()?));
        (worlds[0].map.clone(), Some(worlds))
    };

    let migration_worlds = legacy_snapshot
        .then(|| {
            generate_legacy_demo_worlds()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
                .and_then(|worlds| {
                    let mut worlds = worlds
                        .into_iter()
                        .map(to_world_definition)
                        .collect::<Vec<_>>();
                    worlds.push(legacy_world(parse_legacy_map()?));
                    Ok(worlds)
                })
        })
        .transpose()?;
    let planet_definition = planet
        .map(|planet| ron::to_string(&planet).map(String::into_bytes))
        .transpose()?;

    Ok(WorldComposition {
        seed,
        primary_map,
        worlds,
        migration_worlds,
        planet_definition,
        prepared_geography: prepared_geography
            .map(|geography| geography as Arc<dyn minimium_procgen::PreparedGeography>),
    })
}

fn to_world_definition(world: GeneratedWorld) -> WorldDefinition {
    WorldDefinition {
        instance: world.instance,
        manifest: world.manifest,
        map: world.map,
        portal: world.portal.map(|portal| WorldPortal {
            position: portal.position,
            target_instance: portal.target_instance,
            destination: portal.destination,
        }),
    }
}

fn parse_legacy_map() -> io::Result<WorldMap> {
    minimium_assets::parse_map(SAMPLE_MAP)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn legacy_world(map: WorldMap) -> WorldDefinition {
    let instance = WorldInstanceId::new(3).expect("legacy instance ID is nonzero");
    let generation = GenerationSpec::new(
        WorldSeed::new(0),
        GenerationRevision::new(1).expect("legacy generator revision is nonzero"),
        1,
    )
    .expect("legacy position quantum is nonzero");
    let manifest = WorldManifest::new(
        WorldDefinitionId::new(3).expect("legacy definition ID is nonzero"),
        WorldTopology::Dungeon,
        generation,
        WorldContentRevision::new(1).expect("legacy content revision is nonzero"),
        GeneratorDigest::legacy_map_fingerprint(map.fingerprint()),
    );
    WorldDefinition {
        instance,
        manifest,
        map,
        portal: None,
    }
}
