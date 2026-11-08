//! File-backed adapters for explicit, bounded asset parsing APIs.

mod content_cache;
mod error;
mod geography;
mod material_server;

pub use content_cache::{ContentCache, DEFAULT_CACHE_LIMIT_BYTES};
pub use error::{AssetIoError, AssetIoErrorKind};
pub use geography::{open_earth_package, open_earth_package_cancellable, PreparedEarthGeography};
pub use material_server::ContentServer;

use minimium_world::world::WorldMap;
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

/// Opens a map and its relative mesh assets, then delegates decoding to assets.
pub fn load_map(path: &Path) -> Result<WorldMap, AssetIoError> {
    let source = read_bounded(path, minimium_assets::MAX_MAP_BYTES)?;
    let base_dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_base = std::fs::canonicalize(base_dir)
        .map_err(|error| AssetIoError::io("resolve map directory", error))?;
    let mut io_error = None;
    let parsed = minimium_assets::parse_map_with_mesh_loader(&source, |asset_path, max_bytes| {
        let result = canonical_asset_path(&canonical_base, asset_path)
            .and_then(|path| read_bounded(&path, max_bytes));
        result.map_err(|error| {
            let message = error.to_string();
            io_error = Some(error);
            message
        })
    });
    parsed.map_err(|detail| {
        io_error.unwrap_or_else(|| AssetIoError::invalid_format("map or referenced mesh", detail))
    })
}

/// Opens a bounded world-definition file and delegates validation to assets.
pub fn load_world_definition(
    path: &Path,
) -> Result<minimium_assets::WorldDefinition, AssetIoError> {
    let source = read_bounded(path, minimium_assets::MAX_DEFINITION_BYTES)?;
    minimium_assets::parse_world_definition(&source)
        .map_err(|error| AssetIoError::invalid_format("world definition", error))
}

fn canonical_asset_path(base: &Path, relative: &Path) -> Result<PathBuf, AssetIoError> {
    let path = std::fs::canonicalize(base.join(relative)).map_err(|error| {
        AssetIoError::io(format!("resolve mesh asset {}", relative.display()), error)
    })?;
    if !path.starts_with(base) {
        return Err(AssetIoError::invalid_format(
            format!("mesh asset {}", relative.display()),
            "resolves outside the map directory",
        ));
    }
    Ok(path)
}

fn read_bounded(path: &Path, max_bytes: usize) -> Result<String, AssetIoError> {
    let mut source = String::new();
    let file = File::open(path)
        .map_err(|error| AssetIoError::io(format!("open {}", path.display()), error))?;
    let file_len = file
        .metadata()
        .map_err(|error| AssetIoError::io(format!("read metadata {}", path.display()), error))?
        .len();
    if file_len > max_bytes as u64 {
        return Err(AssetIoError::limit_exceeded(
            format!("file {}", path.display()),
            max_bytes,
        ));
    }
    file.take((max_bytes.saturating_add(1)) as u64)
        .read_to_string(&mut source)
        .map_err(|error| AssetIoError::io(format!("read {}", path.display()), error))?;
    if source.len() > max_bytes {
        return Err(AssetIoError::limit_exceeded(
            format!("file {}", path.display()),
            max_bytes,
        ));
    }
    Ok(source)
}
mod land_cover;
pub mod material_http;
pub mod materials;
