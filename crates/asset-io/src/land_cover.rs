use memmap2::Mmap;
use std::{fs::File, path::Path};

pub(crate) fn open(path: &Path) -> Result<Option<Mmap>, String> {
    let file = match File::open(path.join("worldcover.bin")) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if file.metadata().map_err(|e| e.to_string())?.len()
        != minimium_assets::land_cover::BYTES as u64
    {
        return Err("wrong WorldCover grid size".into());
    }
    // Prepared geography is immutable while mapped, as with GEBCO tiles.
    let data = unsafe { Mmap::map(&file) }.map_err(|e| e.to_string())?;
    let hash = super::materials::read_bounded(&path.join("worldcover.sha256"), 64)?;
    if std::str::from_utf8(&hash).map_err(|e| e.to_string())?
        != minimium_assets::terrain_material::digest(&data)
    {
        return Err("WorldCover grid checksum mismatch".into());
    }
    minimium_assets::land_cover::LandCover::parse(&data)?;
    Ok(Some(data))
}
