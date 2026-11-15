//! A published package owns new files, never mutable source paths or hardlinks.
use super::{emit, DomainLogger, Level};
use crate::{
    materials::{self, MaterialPackage},
    AssetIoError,
};
use minimium_assets::terrain_material::{
    decode_hash, digest, MaterialManifest, MAX_MANIFEST_BYTES,
};
use std::{
    collections::HashMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(super) struct PublishedPackage {
    files: HashMap<String, (PathBuf, u64)>,
    hash: [u8; 32],
    directory: SnapshotDirectory,
}

impl PublishedPackage {
    pub(super) fn create(root: &Path, logger: DomainLogger) -> Result<Self, AssetIoError> {
        let source =
            materials::read_bounded(&root.join("manifest.ron"), MAX_MANIFEST_BYTES as u64)?;
        let manifest = MaterialManifest::parse(&source)
            .map_err(|error| AssetIoError::invalid_format("published material manifest", error))?;
        let hash = decode_hash(&digest(&source))
            .map_err(|error| AssetIoError::invalid_format("published material digest", error))?;
        let directory = SnapshotDirectory::create(logger)
            .map_err(|error| AssetIoError::io("create material snapshot", error))?;
        let mut snapshot = Self {
            files: HashMap::new(),
            hash,
            directory,
        };
        snapshot.write("manifest.ron", &source)?;
        // Retain at most one existing bounded variant in memory. Validation and
        // copying use the same byte buffers even if source files are overwritten.
        for compressed in [true, false] {
            let variant = usize::from(!compressed);
            let mut maps = [Vec::new(), Vec::new(), Vec::new()];
            for (bytes, file) in maps.iter_mut().zip(&manifest.variants[variant].maps) {
                *bytes = materials::read_bounded(&root.join(&file.name), file.bytes)?;
            }
            let package = MaterialPackage::from_parts(&source, compressed, maps)?;
            for (bytes, file) in package.maps.iter().zip(&manifest.variants[variant].maps) {
                snapshot.write(&file.name, bytes)?;
            }
        }
        Ok(snapshot)
    }

    fn write(&mut self, name: &str, bytes: &[u8]) -> Result<(), AssetIoError> {
        // Manifest validation permits only flat, unique filenames. The other
        // caller uses the fixed manifest name; no caller supplies an arbitrary path.
        let path = self
            .directory
            .path
            .as_ref()
            .expect("snapshot is being constructed")
            .join(name);
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| AssetIoError::io("create material snapshot file", error))?;
        self.directory.files.push(path.clone());
        output
            .write_all(bytes)
            .map_err(|error| AssetIoError::io("write material snapshot file", error))?;
        drop(output);
        self.files
            .insert(name.to_owned(), (path, bytes.len() as u64));
        Ok(())
    }

    pub(super) fn files(&self) -> &HashMap<String, (PathBuf, u64)> {
        &self.files
    }
    pub(super) fn hash(&self) -> [u8; 32] {
        self.hash
    }

    pub(super) fn cleanup(&mut self) -> io::Result<()> {
        self.directory.cleanup()
    }
}

struct SnapshotDirectory {
    path: Option<PathBuf>,
    files: Vec<PathBuf>,
    logger: DomainLogger,
}

impl SnapshotDirectory {
    fn create(logger: DomainLogger) -> io::Result<Self> {
        let temporary_root = fs::canonicalize(std::env::temp_dir())?;
        for _ in 0..8 {
            let random = minimium_platform::endpoint::new_random_bytes::<24>()?;
            let suffix: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
            let path = temporary_root.join(format!("minimium-material-{suffix}"));
            let created = {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::DirBuilderExt;
                    fs::DirBuilder::new().mode(0o700).create(&path)
                }
                #[cfg(not(unix))]
                {
                    fs::create_dir(&path)
                }
            };
            match created {
                Ok(()) => {
                    return Ok(Self {
                        path: Some(path),
                        files: Vec::new(),
                        logger,
                    })
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "material snapshot directory collision",
        ))
    }

    fn cleanup(&mut self) -> io::Result<()> {
        let mut failure = None;
        self.files.retain(|path| match fs::remove_file(path) {
            Ok(()) => false,
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => {
                failure.get_or_insert(error);
                true
            }
        });
        if let Some(path) = &self.path {
            // Never recursively traverse or delete anything not created here.
            match fs::remove_dir(path) {
                Ok(()) => self.path = None,
                Err(error) if error.kind() == io::ErrorKind::NotFound => self.path = None,
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        match failure {
            Some(error) => {
                let error = AssetIoError::io("remove material snapshot", error);
                emit(
                    &self.logger,
                    Level::Error,
                    "asset_io.material_snapshot_cleanup_failed",
                    "reason",
                    &error.to_string(),
                );
                Err(io::Error::other(error))
            }
            None => Ok(()),
        }
    }
}

impl Drop for SnapshotDirectory {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
