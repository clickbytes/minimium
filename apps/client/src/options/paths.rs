use super::ClientOptions;
use minimium_commands::{CommandError, CommandErrorCode};
use std::path::PathBuf;

pub(super) fn mapped_local_server_environment() -> std::collections::BTreeMap<String, String> {
    let mut mapped = [
        "MINIMIUM_SERVER_THREADS",
        "MINIMIUM_SERVER_DATA_DIR",
        "MINIMIUM_SERVER_BIND",
        "MINIMIUM_SERVER_GAME_PORT",
        "MINIMIUM_SERVER_MAX_PLAYERS",
        "MINIMIUM_SERVER_AUTOSAVE_INTERVAL",
        "MINIMIUM_SERVER_HEADLESS",
        "MINIMIUM_SERVER_SEED",
        "MINIMIUM_SERVER_PLANET_CONFIG",
        "MINIMIUM_SERVER_CLIMATE_RELIEF_INFLUENCE",
        "MINIMIUM_EARTH_PACKAGE",
    ]
    .into_iter()
    .filter_map(|name| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.is_empty())
            .map(|value| (name.to_owned(), value))
    })
    .collect::<std::collections::BTreeMap<_, _>>();
    for (source, target) in [
        ("MINIMIUM_LOCAL_SEED", "MINIMIUM_SERVER_SEED"),
        (
            "MINIMIUM_LOCAL_PLANET_CONFIG",
            "MINIMIUM_SERVER_PLANET_CONFIG",
        ),
        (
            "MINIMIUM_LOCAL_CLIMATE_RELIEF_INFLUENCE",
            "MINIMIUM_SERVER_CLIMATE_RELIEF_INFLUENCE",
        ),
    ] {
        if !mapped.contains_key(target) {
            if let Some(value) = std::env::var(source).ok().filter(|value| !value.is_empty()) {
                mapped.insert(target.to_owned(), value);
            }
        }
    }
    mapped
}

pub(super) fn resolve_client_paths(options: &mut ClientOptions) -> Result<(), CommandError> {
    let mut selected: Vec<(&'static str, String, PathBuf)> = Vec::new();
    macro_rules! capture_option {
        ($field:ident) => {
            if let Some(path) = options.$field.as_ref() {
                selected.push(("option", stringify!($field).to_owned(), path.clone()));
            }
        };
    }
    capture_option!(map_path);
    capture_option!(world_definition_path);
    capture_option!(profile_dir);
    capture_option!(log_dir);
    capture_option!(server_data_dir);
    capture_option!(config_path);
    capture_option!(server_config_path);
    selected.push(("option", "cache_dir".to_owned(), options.cache_dir.clone()));
    selected.push((
        "option",
        "earth_package_dir".to_owned(),
        options.earth_package_dir.clone(),
    ));
    for key in [
        "server.data_dir",
        "server.material_package",
        "world.map",
        "world.definition",
        "world.planet_config",
        "world.time.initial_config",
        "world.climate.initial_scenario",
    ] {
        if let Some(value) = options.server_settings.get(key) {
            selected.push(("setting", key.to_owned(), PathBuf::from(value)));
        }
    }
    for key in ["MINIMIUM_SERVER_DATA_DIR", "MINIMIUM_SERVER_PLANET_CONFIG"] {
        if let Some(value) = options.server_environment.get(key) {
            selected.push(("environment", key.to_owned(), PathBuf::from(value)));
        }
    }

    let paths = selected
        .iter()
        .map(|(_, _, path)| path.as_path())
        .collect::<Vec<_>>();
    let working_dir = options.working_dir.as_deref().ok_or_else(|| {
        CommandError::new(
            CommandErrorCode::InvalidValue,
            Some("paths"),
            "process working directory was not captured",
            Some("start the client through ClientOptions::parse"),
        )
    })?;
    let resolved =
        minimium_platform::app_paths::AppPaths::resolve_selected_from(working_dir, &paths);
    for ((owner, key, _), path) in selected.into_iter().zip(resolved) {
        match owner {
            "option" => match key.as_str() {
                "map_path" => options.map_path = Some(path),
                "world_definition_path" => options.world_definition_path = Some(path),
                "profile_dir" => options.profile_dir = Some(path),
                "log_dir" => options.log_dir = Some(path),
                "server_data_dir" => options.server_data_dir = Some(path),
                "config_path" => options.config_path = Some(path),
                "server_config_path" => options.server_config_path = Some(path),
                "cache_dir" => options.cache_dir = path,
                "earth_package_dir" => options.earth_package_dir = path,
                _ => unreachable!("only captured path options are assigned"),
            },
            "setting" => {
                options
                    .server_settings
                    .insert(key, path.to_string_lossy().into_owned());
            }
            "environment" => {
                options
                    .server_environment
                    .insert(key, path.to_string_lossy().into_owned());
            }
            _ => unreachable!("only captured path owners are assigned"),
        }
    }
    Ok(())
}
