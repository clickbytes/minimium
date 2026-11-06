//! Read-only GEBCO 2026 ice-surface package shared by server and client.
mod bounds;
use memmap2::Mmap;
use minimium_procgen::EarthSampleError;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    fmt,
    fs::{self, File},
    path::Path,
    sync::{Arc, OnceLock},
};

const TILE_EDGE: usize = 21_600;
const TILE_BYTES: usize = TILE_EDGE * TILE_EDGE * 2;
const LITE_WIDTH: usize = 21_600;
const LITE_HEIGHT: usize = 10_800;
const LITE_SOURCE: &str = "https://data.source.coop/ausantarctic/gebco/GEBCO_2026.tif";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageIndex {
    version: u32,
    edition: String,
    #[serde(default)]
    source_sha256: String,
    #[serde(default)]
    source_url: String,
    #[serde(default)]
    source_etag: String,
    #[serde(default)]
    source_version: String,
    tile_sha256: Vec<String>,
}

pub struct PreparedEarthGeography {
    land_cover: Option<Mmap>,
    package_digest: [u8; 32],
    tiles: Vec<Mmap>,
    width: usize,
    height: usize,
    maxima: OnceLock<Box<[std::sync::atomic::AtomicI32]>>,
}

pub fn open_earth_package(
    path: &Path,
    expected: Option<[u8; 32]>,
) -> Result<Arc<PreparedEarthGeography>, String> {
    let source = fs::read_to_string(path.join("index.ron"))
        .map_err(|error| format!("GEBCO index: {error}"))?;
    let index: PackageIndex =
        ron::from_str(&source).map_err(|error| format!("GEBCO index: {error}"))?;
    let lite = index.version == 2;
    if !(index.version == 1
        && index.edition == "GEBCO_2026_ice_surface"
        && index.tile_sha256.len() == 8
        || lite
            && index.edition == "GEBCO_2026_ice_surface_60arcsec"
            && index.tile_sha256.len() == 1
            && index.source_url == LITE_SOURCE
            && !index.source_etag.is_empty()
            && !index.source_version.is_empty())
    {
        return Err("unsupported GEBCO package index".into());
    }
    let mut package_hash = Sha256::new();
    let (width, height, tile_names) = if lite {
        package_hash.update(b"GEBCO_2026_ice_surface_60arcsec\0");
        for value in [&index.source_url, &index.source_etag, &index.source_version] {
            package_hash.update(value.as_bytes());
            package_hash.update([0]);
        }
        (LITE_WIDTH, LITE_HEIGHT, vec![String::from("elevation.bil")])
    } else {
        package_hash.update(decode_sha256(&index.source_sha256)?);
        (
            TILE_EDGE * 4,
            TILE_EDGE * 2,
            (0..2)
                .flat_map(|y| (0..4).map(move |x| format!("tile-{y}-{x}.bil")))
                .collect(),
        )
    };
    let mut tiles = Vec::with_capacity(tile_names.len());
    for (index_in_package, name) in tile_names.iter().enumerate() {
        let file =
            File::open(path.join(name)).map_err(|error| format!("GEBCO tile {name}: {error}"))?;
        let expected_bytes = if lite {
            LITE_WIDTH * LITE_HEIGHT * 2
        } else {
            TILE_BYTES
        };
        if file.metadata().map_err(|error| error.to_string())?.len() != expected_bytes as u64 {
            return Err(format!("GEBCO tile {name} has the wrong size"));
        }
        // The package is immutable while mapped. The importer checks the
        // byte order and writes a new directory before publishing it.
        let tile = unsafe { Mmap::map(&file) }.map_err(|error| error.to_string())?;
        let actual = Sha256::digest(&tile);
        if actual.as_slice() != decode_sha256(&index.tile_sha256[index_in_package])? {
            return Err(format!("GEBCO tile {name} checksum mismatch"));
        }
        package_hash.update(actual);
        tiles.push(tile);
    }
    let package = PreparedEarthGeography {
        land_cover: crate::land_cover::open(path)?,
        package_digest: package_hash.finalize().into(),
        tiles,
        width,
        height,
        maxima: OnceLock::new(),
    };
    let digest = package.package_digest;
    if expected.is_some_and(|expected| expected != digest) {
        return Err("GEBCO package checksum differs from world definition".into());
    }
    let _ = digest;
    Ok(Arc::new(package))
}

impl std::fmt::Debug for PreparedEarthGeography {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedEarthGeography")
            .field("package_digest", &self.package_digest)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

fn decode_sha256(hex: &str) -> Result<[u8; 32], String> {
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("invalid GEBCO SHA-256 digest".into());
    }
    let mut bytes = [0_u8; 32];
    for (byte, pair) in bytes
        .iter_mut()
        .zip(hex.as_bytes().as_chunks::<2>().0.iter())
    {
        let pair = std::str::from_utf8(pair).map_err(|error| error.to_string())?;
        *byte = u8::from_str_radix(pair, 16).map_err(|error| error.to_string())?;
    }
    Ok(bytes)
}

/// Samples unscaled, bilinearly interpolated GEBCO meters without panicking.
impl PreparedEarthGeography {
    pub fn try_earth_elevation_m(&self, direction: [f64; 3]) -> Result<f64, EarthSampleError> {
        let length = direction
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        if !length.is_finite() || length == 0.0 || !direction.iter().all(|value| value.is_finite())
        {
            return Err(EarthSampleError::InvalidDirection);
        }
        let latitude = (direction[1] / length).asin().to_degrees();
        let longitude = direction[2].atan2(direction[0]).to_degrees();
        let samples_per_degree = self.width as f64 / 360.0;
        let grid_x =
            (((longitude + 180.0) * samples_per_degree - 0.5) * 1_000_000.0).round() / 1_000_000.0;
        let grid_y =
            (((90.0 - latitude) * samples_per_degree - 0.5) * 1_000_000.0).round() / 1_000_000.0;
        let x0 = grid_x.floor() as i64;
        let y0 = grid_y.floor() as i64;
        let fx = grid_x - x0 as f64;
        let fy = grid_y - y0 as f64;
        let h00 = f64::from(self.pixel(x0, y0));
        let h10 = f64::from(self.pixel(x0 + 1, y0));
        let h01 = f64::from(self.pixel(x0, y0 + 1));
        let h11 = f64::from(self.pixel(x0 + 1, y0 + 1));
        let height_m =
            (h00 * (1.0 - fx) + h10 * fx) * (1.0 - fy) + (h01 * (1.0 - fx) + h11 * fx) * fy;
        if height_m.is_finite() {
            Ok(height_m)
        } else {
            Err(EarthSampleError::InvalidDirection)
        }
    }

    pub fn earth_elevation_upper_bound_m(
        &self,
        direction: [f64; 3],
        angle: f64,
    ) -> Result<f64, EarthSampleError> {
        bounds::earth_elevation_upper_bound_m(self, direction, angle)
    }
}

impl PreparedEarthGeography {
    fn pixel(&self, x: i64, y: i64) -> i16 {
        let x = x.rem_euclid(self.width as i64) as usize;
        let y = y.clamp(0, self.height as i64 - 1) as usize;
        let (tile, offset) = if self.tiles.len() == 1 {
            (&self.tiles[0], (y * self.width + x) * 2)
        } else {
            (
                &self.tiles[(y / TILE_EDGE) * 4 + x / TILE_EDGE],
                ((y % TILE_EDGE) * TILE_EDGE + x % TILE_EDGE) * 2,
            )
        };
        i16::from_le_bytes([tile[offset], tile[offset + 1]])
    }
}

impl minimium_procgen::PreparedGeography for PreparedEarthGeography {
    fn land_cover(&self, direction: [f64; 3]) -> Option<[f32; 6]> {
        minimium_assets::land_cover::LandCover::parse(self.land_cover.as_ref()?)
            .ok()?
            .sample(direction)
    }
    fn package_digest(&self) -> [u8; 32] {
        self.package_digest
    }

    fn elevation_m(&self, direction: [f64; 3]) -> Result<f64, EarthSampleError> {
        self.try_earth_elevation_m(direction)
    }

    fn elevation_upper_bound_m(
        &self,
        direction: [f64; 3],
        angle: f64,
    ) -> Result<f64, EarthSampleError> {
        self.earth_elevation_upper_bound_m(direction, angle)
    }
}
