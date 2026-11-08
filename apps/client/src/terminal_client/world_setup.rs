use crate::{messages::ClientText as Text, options::ClientOptions};
use minimium_diagnostics::DomainLogger;
use minimium_localization::{Locale, Localizer};
use minimium_world::world::{WorldInstanceId, WorldManifest, WorldMap};
use std::{collections::HashMap, error::Error, io};

pub(super) struct WorldSetup {
    pub map: WorldMap,
    pub world_catalog: HashMap<WorldInstanceId, WorldMap>,
    pub world_manifests: HashMap<WorldInstanceId, WorldManifest>,
}

pub(super) fn resolve(
    options: &ClientOptions,
    asset_io_logger: &DomainLogger,
    procgen_logger: &DomainLogger,
    localizer: &Localizer,
    locale: Locale,
) -> Result<WorldSetup, Box<dyn Error>> {
    let (map, world_catalog, world_manifests) = if let Some(path) = options.map_path.as_deref() {
        let map = minimium_asset_io::load_map(path).map_err(|error| {
            crate::diagnostics::failure(asset_io_logger, error.code(), &error.to_string());
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .localized(localizer, locale)
        })?;
        let instance = WorldInstanceId::new(1).expect("authored world instance is nonzero");
        let manifest = crate::world_content::authored_manifest(instance.get(), &map);
        (
            map.clone(),
            HashMap::from([(instance, map)]),
            HashMap::from([(instance, manifest)]),
        )
    } else if let Some(path) = options.world_definition_path.as_deref() {
        let definition = minimium_asset_io::load_world_definition(path).map_err(|error| {
            crate::diagnostics::failure(asset_io_logger, error.code(), &error.to_string());
            Text::MapLoadFailed {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
            .localized(localizer, locale)
        })?;
        minimium_procgen::validate_world_manifest(definition.manifest()).map_err(|error| {
            crate::diagnostics::failure(procgen_logger, error.code(), &error.to_string());
            io::Error::new(io::ErrorKind::InvalidData, error)
        })?;
        let map =
            minimium_procgen::materialize_dungeon_map(definition.dungeon()).map_err(|error| {
                crate::diagnostics::failure(procgen_logger, error.code(), &error.to_string());
                Text::BundledMapInvalid(error.to_string()).localized(localizer, locale)
            })?;
        let instance = WorldInstanceId::new(4).expect("manual world instance ID is nonzero");
        (
            map.clone(),
            HashMap::from([(instance, map)]),
            HashMap::from([(instance, definition.manifest())]),
        )
    } else {
        let worlds = minimium_game_sample::generate_demo_worlds().map_err(|error| {
            crate::diagnostics::failure(procgen_logger, error.code(), &error.to_string());
            Text::BundledMapInvalid(error.to_string()).localized(localizer, locale)
        })?;
        let mut catalog = HashMap::new();
        let mut manifests = HashMap::new();
        for world in &worlds {
            catalog.insert(world.instance, world.map.clone());
            manifests.insert(world.instance, world.manifest);
        }
        (worlds[0].map.clone(), catalog, manifests)
    };
    Ok(WorldSetup {
        map,
        world_catalog,
        world_manifests,
    })
}
