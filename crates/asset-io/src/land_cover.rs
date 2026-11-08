use crate::AssetIoError;
use memmap2::Mmap;
use sha2::{Digest, Sha256};
use std::{fs::File, path::Path};

pub(crate) fn open_cancellable(
    path: &Path,
    cancelled: impl Fn() -> bool,
) -> Result<Option<Mmap>, AssetIoError> {
    if cancelled() {
        return Err(AssetIoError::cancelled("WorldCover preparation"));
    }
    let file = match File::open(path.join("worldcover.bin")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AssetIoError::io("open WorldCover grid", error)),
    };
    if file
        .metadata()
        .map_err(|error| AssetIoError::io("WorldCover grid metadata", error))?
        .len()
        != minimium_assets::land_cover::BYTES as u64
    {
        return Err(AssetIoError::limit_exceeded(
            "WorldCover grid size",
            minimium_assets::land_cover::BYTES,
        ));
    }
    // Prepared geography is immutable while mapped, as with GEBCO tiles.
    let data = unsafe { Mmap::map(&file) }
        .map_err(|error| AssetIoError::io("map WorldCover grid", error))?;
    let hash = super::materials::read_bounded(&path.join("worldcover.sha256"), 64)?;
    let expected = std::str::from_utf8(&hash)
        .map_err(|error| {
            AssetIoError::caused_by(
                crate::AssetIoErrorKind::InvalidFormat,
                "WorldCover digest",
                error,
            )
        })?
        .trim();
    let mut digest = Sha256::new();
    for chunk in data.chunks(8 * 1024 * 1024) {
        if cancelled() {
            return Err(AssetIoError::cancelled("WorldCover hashing"));
        }
        digest.update(chunk);
    }
    if cancelled() {
        return Err(AssetIoError::cancelled("WorldCover hashing"));
    }
    if format!("{:x}", digest.finalize()) != expected {
        return Err(AssetIoError::digest_mismatch("WorldCover grid"));
    }
    minimium_assets::land_cover::LandCover::parse(&data)
        .map_err(|error| AssetIoError::invalid_format("WorldCover grid", error))?;
    Ok(Some(data))
}
