//! Hash-addressed, transactional material installation. Hosts choose transport/workers.
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
}

impl MaterialPackage {
    pub fn open(path: &Path, compressed: bool) -> Result<Self, String> {
        let source = read_bounded(&path.join("manifest.ron"), MAX_MANIFEST_BYTES as u64)?;
        let manifest = MaterialManifest::parse(&source)?;
        let variant = usize::from(!compressed);
        let mut maps = [Vec::new(), Vec::new(), Vec::new()];
        for (data, file) in maps.iter_mut().zip(&manifest.variants[variant].maps) {
            *data = read_bounded(&path.join(&file.name), file.bytes)?;
        }
        Self::from_parts(&source, compressed, maps)
    }

    pub fn from_parts(source: &[u8], compressed: bool, maps: [Vec<u8>; 3]) -> Result<Self, String> {
        let manifest = MaterialManifest::parse(source)?;
        let variant = usize::from(!compressed);
        let selected = &manifest.variants[variant];
        let mut gpu_bytes = 0;
        for (index, (data, file)) in maps.iter().zip(&selected.maps).enumerate() {
            if data.len() as u64 != file.bytes || digest(data) != file.sha256 {
                return Err(format!("material checksum mismatch: {}", file.name));
            }
            let format = if compressed {
                [132, 141, 137][index]
            } else {
                [43, 37, 37][index]
            };
            gpu_bytes += ktx2::Texture::parse(data, selected.edge, format)?.gpu_bytes;
        }
        if gpu_bytes > MAX_PACKAGE_BYTES {
            return Err("material GPU budget exceeded".into());
        }
        Ok(Self {
            manifest,
            manifest_hash: digest(source),
            variant,
            maps,
        })
    }
}

pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > limit {
        return Err("asset exceeds size limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("asset exceeds size limit".into());
    }
    Ok(bytes)
}

/// Downloads only absent hashes. Temporary files never replace a verified blob.
pub fn install(
    cache: &Path,
    manifest_bytes: &[u8],
    expected_hash: &str,
    compressed: bool,
    mut fetch: impl FnMut(&str, u64, &mut dyn Write) -> Result<(), String>,
) -> Result<MaterialPackage, String> {
    decode_hash(expected_hash)?;
    if digest(manifest_bytes) != expected_hash {
        return Err("material manifest checksum mismatch".into());
    }
    let manifest = MaterialManifest::parse(manifest_bytes)?;
    fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let variant = usize::from(!compressed);
    let mut maps = [Vec::new(), Vec::new(), Vec::new()];
    for (data, file) in maps.iter_mut().zip(&manifest.variants[variant].maps) {
        let blob = cache.join(&file.sha256);
        if let Ok(existing) = read_bounded(&blob, file.bytes) {
            if existing.len() as u64 == file.bytes && digest(&existing) == file.sha256 {
                *data = existing;
                continue;
            }
        }
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let temporary = cache.join(format!(
            "{}.{}.{nonce}.part",
            file.sha256,
            std::process::id()
        ));
        let result = (|| {
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)
                .map_err(|e| e.to_string())?;
            let mut writer = LimitedWriter {
                output: &mut output,
                remaining: file.bytes,
            };
            fetch(&file.name, file.bytes, &mut writer)?;
            output.sync_all().map_err(|e| e.to_string())?;
            *data = read_bounded(&temporary, file.bytes)?;
            if data.len() as u64 != file.bytes || digest(data) != file.sha256 {
                return Err("download checksum mismatch".into());
            }
            // A corrupt existing blob is never considered installed.
            if blob.exists() {
                if let Ok(existing) = read_bounded(&blob, file.bytes) {
                    if existing.len() as u64 == file.bytes && digest(&existing) == file.sha256 {
                        fs::remove_file(&temporary).map_err(|e| e.to_string())?;
                        return Ok(());
                    }
                }
                fs::remove_file(&blob).map_err(|e| e.to_string())?;
            }
            fs::rename(&temporary, &blob).map_err(|e| e.to_string())
        })();
        if temporary.exists() {
            let _ = fs::remove_file(temporary);
        }
        result?;
    }
    let package = MaterialPackage::from_parts(manifest_bytes, compressed, maps)?;
    // Immutable receipt becomes visible only after every file has passed validation.
    let receipt: PathBuf = cache.join(format!("{expected_hash}-{variant}.ron"));
    if !receipt.exists() {
        let temporary = receipt.with_extension(format!("{}.part", std::process::id()));
        fs::write(&temporary, manifest_bytes).map_err(|e| e.to_string())?;
        fs::rename(temporary, receipt).map_err(|e| e.to_string())?;
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
