//! Process-level resolved paths shared with runtime adapters.
//!
//! Path selection belongs to an application. This type only anchors the
//! selected paths to one captured working directory so consumers do not
//! independently interpret relative paths later in the process lifetime.

use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppPaths {
    pub profile_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config_dir: PathBuf,
    pub log_dir: PathBuf,
}

impl AppPaths {
    /// Resolve one application-selected path against the current working directory.
    pub fn resolve_path(path: &Path) -> io::Result<PathBuf> {
        Ok(Self::resolve_selected(&[path])?.remove(0))
    }

    /// Resolve several selected paths using the same captured working directory.
    pub fn resolve_selected(paths: &[&Path]) -> io::Result<Vec<PathBuf>> {
        let working_dir = std::env::current_dir()?;
        Ok(Self::resolve_selected_from(&working_dir, paths))
    }

    /// Resolve several paths against an already captured process directory.
    pub fn resolve_selected_from(working_dir: &Path, paths: &[&Path]) -> Vec<PathBuf> {
        paths
            .iter()
            .map(|path| absolute_from(&working_dir, path))
            .collect()
    }

    /// Resolve selected paths against one process working directory.
    ///
    /// The application supplies the existing data/cache/config defaults;
    /// logs default to `./logs` from the captured working directory.
    pub fn resolve(
        profile_dir: &Path,
        data_dir: &Path,
        cache_dir: &Path,
        config_dir: &Path,
        log_dir: Option<&Path>,
    ) -> io::Result<Self> {
        let log_dir = log_dir.unwrap_or_else(|| Path::new("logs"));
        let working_dir = std::env::current_dir()?;
        Ok(Self::resolve_from(
            &working_dir,
            profile_dir,
            data_dir,
            cache_dir,
            config_dir,
            Some(log_dir),
        ))
    }

    /// Build a complete path set against an already captured process directory.
    pub fn resolve_from(
        working_dir: &Path,
        profile_dir: &Path,
        data_dir: &Path,
        cache_dir: &Path,
        config_dir: &Path,
        log_dir: Option<&Path>,
    ) -> Self {
        let log_dir = log_dir.unwrap_or_else(|| Path::new("logs"));
        let resolved = Self::resolve_selected_from(
            working_dir,
            &[profile_dir, data_dir, cache_dir, config_dir, log_dir],
        );
        Self {
            profile_dir: resolved[0].clone(),
            data_dir: resolved[1].clone(),
            cache_dir: resolved[2].clone(),
            config_dir: resolved[3].clone(),
            log_dir: resolved[4].clone(),
        }
    }
}

fn absolute_from(working_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        working_dir.join(path)
    }
}
