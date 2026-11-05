//! File-backed adapters for explicit, bounded asset parsing APIs.

mod geography;

pub use geography::{open_earth_package, PreparedEarthGeography};

use minimium_world::world::WorldMap;
use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
};

/// Opens a map and its relative mesh assets, then delegates decoding to assets.
pub fn load_map(path: &Path) -> io::Result<WorldMap> {
    let source = read_bounded(path, minimium_assets::MAX_MAP_BYTES)?;
    let base_dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_base = std::fs::canonicalize(base_dir)?;
    minimium_assets::parse_map_with_mesh_loader(&source, |asset_path, max_bytes| {
        let canonical_asset = canonical_asset_path(&canonical_base, asset_path)?;
        read_bounded(&canonical_asset, max_bytes)
            .map_err(|error| format!("cannot read mesh asset `{}`: {error}", asset_path.display()))
    })
    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Opens a bounded world-definition file and delegates validation to assets.
pub fn load_world_definition(path: &Path) -> io::Result<minimium_assets::WorldDefinition> {
    let source = read_bounded(path, minimium_assets::MAX_DEFINITION_BYTES)?;
    minimium_assets::parse_world_definition(&source)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn canonical_asset_path(base: &Path, relative: &Path) -> Result<PathBuf, String> {
    let path = std::fs::canonicalize(base.join(relative)).map_err(|error| {
        format!(
            "cannot resolve mesh asset `{}`: {error}",
            relative.display()
        )
    })?;
    if !path.starts_with(base) {
        return Err(format!(
            "mesh asset `{}` resolves outside the map directory",
            relative.display()
        ));
    }
    Ok(path)
}

fn read_bounded(path: &Path, max_bytes: usize) -> io::Result<String> {
    let mut source = String::new();
    File::open(path)?
        .take((max_bytes.saturating_add(1)) as u64)
        .read_to_string(&mut source)?;
    Ok(source)
}
