//! Bounded, hash-addressed cache ownership for downloaded content.
use crate::{materials, AssetIoError, AssetIoErrorKind};
use minimium_assets::terrain_material::MaterialManifest;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_CACHE_LIMIT_BYTES: u64 = 8 * 1024 * 1024 * 1024;

pub struct ContentCache {
    root: PathBuf,
    maximum_bytes: u64,
    run_id: String,
    next_task: AtomicU64,
}

impl ContentCache {
    pub fn new(root: PathBuf, maximum_bytes: u64) -> Result<Self, AssetIoError> {
        if maximum_bytes == 0 {
            return Err(AssetIoError::limit_exceeded("content cache capacity", 1));
        }
        fs::create_dir_all(&root)
            .map_err(|error| AssetIoError::io("create content cache", error))?;
        fs::create_dir_all(root.join(".leases"))
            .map_err(|error| AssetIoError::io("create content cache lease directory", error))?;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| {
                AssetIoError::caused_by(AssetIoErrorKind::Io, "read cache run clock", error)
            })?
            .as_nanos();
        Ok(Self {
            root,
            maximum_bytes,
            run_id: format!("{}-{timestamp}", std::process::id()),
            next_task: AtomicU64::new(1),
        })
    }

    pub fn with_default_limit(root: PathBuf) -> Result<Self, AssetIoError> {
        Self::new(root, DEFAULT_CACHE_LIMIT_BYTES)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Serializes capacity reservation across processes, then installs and
    /// verifies a package before returning a lease that protects its blobs.
    pub fn install(
        &self,
        manifest_bytes: &[u8],
        expected_hash: &str,
        compressed: bool,
        fetch: impl FnMut(&str, u64, &mut dyn std::io::Write) -> Result<(), AssetIoError>,
    ) -> Result<materials::MaterialPackage, AssetIoError> {
        let _reservation =
            minimium_platform::files::lock_file(&self.root.join(".quota.lock"), true)
                .map_err(|error| AssetIoError::io("reserve content cache space", error))?;
        self.remove_abandoned_staging()?;
        let manifest = MaterialManifest::parse(manifest_bytes)
            .map_err(|error| AssetIoError::invalid_format("material manifest", error))?;
        let variant = usize::from(!compressed);
        let required = manifest.variants[variant]
            .maps
            .iter()
            .try_fold(0_u64, |total, file| {
                let blob = self.root.join(&file.sha256);
                let installed = materials::read_bounded(&blob, file.bytes).is_ok_and(|bytes| {
                    bytes.len() as u64 == file.bytes
                        && minimium_assets::terrain_material::digest(&bytes) == file.sha256
                });
                if installed {
                    Ok(total)
                } else {
                    total.checked_add(file.bytes).ok_or_else(|| {
                        AssetIoError::limit_exceeded("content package size", self.maximum_bytes)
                    })
                }
            })?;
        self.make_room(required)?;
        let mut update_locks = std::collections::HashMap::new();
        for file in &manifest.variants[variant].maps {
            let blob = self.root.join(&file.sha256);
            let valid = materials::read_bounded(&blob, file.bytes).is_ok_and(|bytes| {
                bytes.len() as u64 == file.bytes
                    && minimium_assets::terrain_material::digest(&bytes) == file.sha256
            });
            if !valid && !update_locks.contains_key(&file.sha256) {
                let path = self
                    .root
                    .join(".leases")
                    .join(format!("{}.lock", file.sha256));
                let lock = minimium_platform::files::lock_file(&path, true)
                    .map_err(|error| AssetIoError::io("lock material digest for update", error))?;
                update_locks.insert(file.sha256.clone(), lock);
            }
        }
        let task_id = self.next_task.fetch_add(1, Ordering::Relaxed);
        let mut package = materials::install(
            &self.root,
            manifest_bytes,
            expected_hash,
            compressed,
            &self.run_id,
            task_id,
            fetch,
        )?;
        let mut leases = Vec::with_capacity(manifest.variants[variant].maps.len());
        for file in &manifest.variants[variant].maps {
            let lock = if let Some(lock) = update_locks.remove(&file.sha256) {
                minimium_platform::files::downgrade_lock(lock)
            } else {
                let path = self
                    .root
                    .join(".leases")
                    .join(format!("{}.lock", file.sha256));
                minimium_platform::files::lock_file(&path, false)
            }
            .map_err(|error| AssetIoError::io("lease cached material", error))?;
            leases.push(lock);
        }
        package.lease = Some(Arc::new(CacheLease { _locks: leases }));
        for file in &manifest.variants[variant].maps {
            touch(&self.root.join(&file.sha256))?;
        }
        Ok(package)
    }

    fn make_room(&self, required: u64) -> Result<(), AssetIoError> {
        let mut entries = Vec::new();
        let mut used = 0_u64;
        for entry in fs::read_dir(&self.root)
            .map_err(|error| AssetIoError::io("list content cache", error))?
        {
            let entry =
                entry.map_err(|error| AssetIoError::io("read content cache entry", error))?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name == ".quota.lock" || name == ".leases" {
                continue;
            }
            let metadata = entry
                .metadata()
                .map_err(|error| AssetIoError::io("read cached content metadata", error))?;
            if !metadata.is_file() {
                continue;
            }
            let len = metadata.len();
            used = used.saturating_add(len);
            if is_digest_name(name) {
                entries.push((
                    entry.path(),
                    Some(name.to_owned()),
                    len,
                    metadata.modified().ok(),
                ));
            } else if is_receipt_name(name) {
                entries.push((entry.path(), None, len, metadata.modified().ok()));
            }
        }
        if required > self.maximum_bytes {
            return Err(AssetIoError::limit_exceeded(
                "content cache package reservation",
                self.maximum_bytes,
            ));
        }
        entries.sort_by_key(|(_, _, _, modified)| *modified);
        for (path, digest, bytes, _) in entries {
            if used.saturating_add(required) <= self.maximum_bytes {
                break;
            }
            let _lease_guard = if let Some(digest) = digest {
                let lock_path = self.root.join(".leases").join(format!("{digest}.lock"));
                let Some(guard) = minimium_platform::files::try_lock_file(&lock_path)
                    .map_err(|error| AssetIoError::io("check cached content lease", error))?
                else {
                    continue;
                };
                Some(guard)
            } else {
                None
            };
            fs::remove_file(&path)
                .map_err(|error| AssetIoError::io("evict cached content", error))?;
            used = used.saturating_sub(bytes);
        }
        if used.saturating_add(required) > self.maximum_bytes {
            return Err(AssetIoError::limit_exceeded(
                "content cache capacity with active leases",
                self.maximum_bytes,
            ));
        }
        Ok(())
    }

    fn remove_abandoned_staging(&self) -> Result<(), AssetIoError> {
        for entry in fs::read_dir(&self.root)
            .map_err(|error| AssetIoError::io("list content cache staging", error))?
        {
            let entry = entry
                .map_err(|error| AssetIoError::io("read content cache staging entry", error))?;
            if entry
                .file_type()
                .map_err(|error| AssetIoError::io("read staging entry type", error))?
                .is_file()
                && entry
                    .file_name()
                    .to_str()
                    .is_some_and(is_cache_staging_name)
            {
                fs::remove_file(entry.path()).map_err(|error| {
                    AssetIoError::io("remove abandoned cache staging file", error)
                })?;
            }
        }
        Ok(())
    }
}

pub(crate) struct CacheLease {
    _locks: Vec<fs::File>,
}

fn is_digest_name(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn touch(path: &Path) -> Result<(), AssetIoError> {
    let file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error| AssetIoError::io("update cached content last-use time", error))?;
    let modified = std::fs::FileTimes::new().set_modified(SystemTime::now());
    file.set_times(modified)
        .map_err(|error| AssetIoError::io("update cached content last-use time", error))
}

fn is_receipt_name(value: &str) -> bool {
    value
        .strip_suffix(".ron")
        .and_then(|stem| stem.rsplit_once('-'))
        .is_some_and(|(digest, variant)| is_digest_name(digest) && matches!(variant, "0" | "1"))
}

fn is_cache_staging_name(value: &str) -> bool {
    let Some((stem, task)) = value
        .strip_suffix(".part")
        .and_then(|name| name.rsplit_once('.'))
    else {
        return false;
    };
    let Some((owner, run_id)) = stem.rsplit_once('.') else {
        return false;
    };
    let Some((pid, nanos)) = run_id.split_once('-') else {
        return false;
    };
    !task.is_empty()
        && task.bytes().all(|byte| byte.is_ascii_digit())
        && !pid.is_empty()
        && pid.bytes().all(|byte| byte.is_ascii_digit())
        && !nanos.is_empty()
        && nanos.bytes().all(|byte| byte.is_ascii_digit())
        && (is_digest_name(owner)
            || owner.rsplit_once('-').is_some_and(|(digest, variant)| {
                is_digest_name(digest) && matches!(variant, "0" | "1")
            }))
}
