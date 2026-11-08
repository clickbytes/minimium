//! Hash-addressed, transactional material installation. Hosts choose transport/workers.
use crate::{AssetIoError, AssetIoErrorKind};
use minimium_assets::{ktx2, terrain_material::*};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub struct MaterialPackage {
    pub manifest: MaterialManifest,
    pub manifest_hash: String,
    pub variant: usize,
    pub maps: [Vec<u8>; 3],
    pub(crate) lease: Option<std::sync::Arc<super::content_cache::CacheLease>>,
}

impl MaterialPackage {
    pub fn open(path: &Path, compressed: bool) -> Result<Self, AssetIoError> {
        let source = read_bounded(&path.join("manifest.ron"), MAX_MANIFEST_BYTES as u64)?;
        let manifest = MaterialManifest::parse(&source)
            .map_err(|error| AssetIoError::invalid_format("material manifest", error))?;
        let variant = usize::from(!compressed);
        let mut maps = [Vec::new(), Vec::new(), Vec::new()];
        for (data, file) in maps.iter_mut().zip(&manifest.variants[variant].maps) {
            *data = read_bounded(&path.join(&file.name), file.bytes)?;
        }
        Self::from_parts(&source, compressed, maps)
    }

    pub fn from_parts(
        source: &[u8],
        compressed: bool,
        maps: [Vec<u8>; 3],
    ) -> Result<Self, AssetIoError> {
        let manifest = MaterialManifest::parse(source)
            .map_err(|error| AssetIoError::invalid_format("material manifest", error))?;
        let variant = usize::from(!compressed);
        let selected = &manifest.variants[variant];
        let mut gpu_bytes = 0;
        for (index, (data, file)) in maps.iter().zip(&selected.maps).enumerate() {
            if data.len() as u64 != file.bytes || digest(data) != file.sha256 {
                return Err(AssetIoError::digest_mismatch(format!(
                    "material map {}",
                    file.name
                )));
            }
            let format = if compressed {
                [132, 141, 137][index]
            } else {
                [43, 37, 37][index]
            };
            gpu_bytes += ktx2::Texture::parse(data, selected.edge, format)
                .map_err(|error| AssetIoError::invalid_format("KTX2 material map", error))?
                .gpu_bytes;
        }
        if gpu_bytes > MAX_PACKAGE_BYTES {
            return Err(AssetIoError::limit_exceeded(
                "material GPU byte budget",
                MAX_PACKAGE_BYTES,
            ));
        }
        Ok(Self {
            manifest,
            manifest_hash: digest(source),
            variant,
            maps,
            lease: None,
        })
    }
}

pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, AssetIoError> {
    let file = fs::File::open(path)
        .map_err(|error| AssetIoError::io(format!("open asset {}", path.display()), error))?;
    if file
        .metadata()
        .map_err(|error| {
            AssetIoError::io(format!("read asset metadata {}", path.display()), error)
        })?
        .len()
        > limit
    {
        return Err(AssetIoError::limit_exceeded(
            format!("asset {}", path.display()),
            limit,
        ));
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| AssetIoError::io(format!("read asset {}", path.display()), error))?;
    if bytes.len() as u64 > limit {
        return Err(AssetIoError::limit_exceeded(
            format!("asset {}", path.display()),
            limit,
        ));
    }
    Ok(bytes)
}

/// Downloads only absent hashes. Temporary files never replace a verified blob.
pub(crate) fn install(
    cache: &Path,
    manifest_bytes: &[u8],
    expected_hash: &str,
    compressed: bool,
    run_id: &str,
    task_id: u64,
    mut fetch: impl FnMut(&str, u64, &mut dyn Write) -> Result<(), AssetIoError>,
) -> Result<MaterialPackage, AssetIoError> {
    decode_hash(expected_hash)
        .map_err(|error| AssetIoError::invalid_format("expected material digest", error))?;
    if digest(manifest_bytes) != expected_hash {
        return Err(AssetIoError::digest_mismatch("material manifest"));
    }
    let manifest = MaterialManifest::parse(manifest_bytes)
        .map_err(|error| AssetIoError::invalid_format("material manifest", error))?;
    fs::create_dir_all(cache).map_err(|error| AssetIoError::io("create material cache", error))?;
    let variant = usize::from(!compressed);
    let mut maps = [Vec::new(), Vec::new(), Vec::new()];
    for (data, file) in maps.iter_mut().zip(&manifest.variants[variant].maps) {
        let blob = cache.join(&file.sha256);
        match read_bounded(&blob, file.bytes) {
            Ok(existing)
                if existing.len() as u64 == file.bytes && digest(&existing) == file.sha256 =>
            {
                *data = existing;
                continue;
            }
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    AssetIoErrorKind::NotFound | AssetIoErrorKind::LimitExceeded
                ) => {}
            Err(error) => return Err(error),
        }
        let temporary = cache.join(format!("{}.{}.{}.part", file.sha256, run_id, task_id));
        let result = (|| {
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|error| AssetIoError::io("create staged material", error))?;
            let mut writer = LimitedWriter {
                output: &mut output,
                remaining: file.bytes,
            };
            fetch(&file.name, file.bytes, &mut writer)?;
            output
                .sync_all()
                .map_err(|error| AssetIoError::io("sync staged material", error))?;
            *data = read_bounded(&temporary, file.bytes)?;
            if data.len() as u64 != file.bytes || digest(data) != file.sha256 {
                return Err(AssetIoError::digest_mismatch(format!(
                    "downloaded material {}",
                    file.name
                )));
            }
            // A corrupt existing blob is never considered installed.
            if blob.exists() {
                match read_bounded(&blob, file.bytes) {
                    Ok(existing)
                        if existing.len() as u64 == file.bytes
                            && digest(&existing) == file.sha256 =>
                    {
                        fs::remove_file(&temporary).map_err(|error| {
                            AssetIoError::io("remove redundant staged material", error)
                        })?;
                        return Ok(());
                    }
                    Ok(_) => {}
                    Err(error)
                        if matches!(
                            error.kind(),
                            AssetIoErrorKind::NotFound | AssetIoErrorKind::LimitExceeded
                        ) => {}
                    Err(error) => return Err(error),
                }
                fs::remove_file(&blob)
                    .map_err(|error| AssetIoError::io("remove corrupt cached material", error))?;
            }
            fs::rename(&temporary, &blob)
                .map_err(|error| AssetIoError::io("publish cached material", error))
        })();
        if temporary.exists() {
            fs::remove_file(temporary)
                .map_err(|error| AssetIoError::io("remove staged material after failure", error))?;
        }
        result?;
    }
    let package = MaterialPackage::from_parts(manifest_bytes, compressed, maps)?;
    // Immutable receipt becomes visible only after every file has passed validation.
    let receipt: PathBuf = cache.join(format!("{expected_hash}-{variant}.ron"));
    if !receipt.exists() {
        let temporary = receipt.with_extension(format!("{run_id}.{task_id}.part"));
        minimium_platform::files::atomic_write_file(&temporary, &receipt, manifest_bytes)
            .map_err(|error| AssetIoError::io("publish material receipt", error))?;
    }
    Ok(package)
}

struct LimitedWriter<'a> {
    output: &'a mut fs::File,
    remaining: u64,
}
impl Write for LimitedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(io::Error::other("material download exceeds declared size"));
        }
        let written = self.output.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}
