//! Versioned server-side planet preset resolution. `extends` is our own file
//! composition rule; RON itself only parses each individual document.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs, io,
    path::{Path, PathBuf},
};

const MAX_EXTENDS_DEPTH: usize = 8;
const EARTH_RADIUS_MM: u64 = 6_371_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum PlanetMode {
    Procedural,
    Earth,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Preset {
    extends: Option<PathBuf>,
    version: Option<u32>,
    mode: Option<PlanetMode>,
    generator_revision: Option<u32>,
    radius_mm: Option<u64>,
    height_scale_permille: Option<u32>,
    sea_level_mm: Option<i64>,
    mountain_limit_mm: Option<u32>,
    detail_strength_mm: Option<u32>,
    detail_wavelength_mm: Option<u32>,
    mountain_band_frequency: Option<u32>,
    earth_package: Option<PathBuf>,
}

impl Preset {
    fn overlay(&mut self, child: Self) {
        macro_rules! replace {
            ($field:ident) => {
                if child.$field.is_some() {
                    self.$field = child.$field;
                }
            };
        }
        replace!(version);
        replace!(mode);
        replace!(generator_revision);
        replace!(radius_mm);
        replace!(height_scale_permille);
        replace!(sea_level_mm);
        replace!(mountain_limit_mm);
        replace!(detail_strength_mm);
        replace!(detail_wavelength_mm);
        replace!(mountain_band_frequency);
        replace!(earth_package);
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResolvedPlanetConfig {
    pub version: u32,
    #[serde(default = "legacy_generator_revision")]
    pub generator_revision: u32,
    // RON's serializer rejects u128 integers; preserve all 128 bits as decimal text.
    #[serde(with = "seed_string")]
    pub seed: u128,
    pub mode: PlanetMode,
    pub radius_mm: u64,
    pub height_scale_permille: u32,
    pub sea_level_mm: i64,
    pub mountain_limit_mm: u32,
    pub detail_strength_mm: u32,
    pub detail_wavelength_mm: u32,
    pub mountain_band_frequency: u32,
    pub earth_package: Option<PathBuf>,
    pub package_digest: [u8; 32],
}

const fn legacy_generator_revision() -> u32 {
    3
}

mod seed_string {
    use serde::{de::Error, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(seed: &u128, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&seed.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u128, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse::<u128>().map_err(D::Error::custom)
    }
}

impl ResolvedPlanetConfig {
    pub fn procedural(seed: u128) -> Self {
        // The radius is 650 km +/- 5%, selected with an integer hash. This
        // does not depend on platform floating-point rounding.
        let mut value = (seed as u64) ^ ((seed >> 64) as u64);
        value ^= value >> 30;
        value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value ^= value >> 27;
        value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
        value ^= value >> 31;
        Self {
            version: 1,
            generator_revision: 3,
            seed,
            mode: PlanetMode::Procedural,
            radius_mm: 617_500_000 + value % 65_000_001,
            height_scale_permille: 1000,
            sea_level_mm: 0,
            mountain_limit_mm: 5_000_000,
            detail_strength_mm: 15_000,
            detail_wavelength_mm: 30_000,
            mountain_band_frequency: 8,
            earth_package: None,
            package_digest: [0; 32],
        }
    }

    fn from_preset(preset: Preset, seed: u128) -> io::Result<Self> {
        let mut resolved = Self::procedural(seed);
        resolved.version = preset.version.unwrap_or(1);
        resolved.mode = preset.mode.unwrap_or(PlanetMode::Procedural);
        resolved.generator_revision =
            preset
                .generator_revision
                .unwrap_or(if resolved.mode == PlanetMode::Earth {
                    4
                } else {
                    3
                });
        if resolved.mode == PlanetMode::Earth {
            resolved.radius_mm = 650_000_000;
            resolved.height_scale_permille = 100;
        }
        resolved.radius_mm = preset.radius_mm.unwrap_or(resolved.radius_mm);
        resolved.height_scale_permille = preset
            .height_scale_permille
            .unwrap_or(resolved.height_scale_permille);
        resolved.sea_level_mm = preset.sea_level_mm.unwrap_or(0);
        resolved.mountain_limit_mm = preset.mountain_limit_mm.unwrap_or(5_000_000);
        resolved.detail_strength_mm = preset
            .detail_strength_mm
            .unwrap_or(resolved.detail_strength_mm);
        resolved.detail_wavelength_mm = preset
            .detail_wavelength_mm
            .unwrap_or(resolved.detail_wavelength_mm);
        resolved.mountain_band_frequency = preset.mountain_band_frequency.unwrap_or(8);
        resolved.earth_package = preset.earth_package;
        resolved.validate()?;
        Ok(resolved)
    }

    pub fn validate(&self) -> io::Result<()> {
        let invalid = |reason| io::Error::new(io::ErrorKind::InvalidInput, reason);
        if self.version != 1 {
            return Err(invalid("unsupported planet preset version"));
        }
        if !matches!(self.generator_revision, 3 | 4)
            || self.generator_revision == 4 && self.mode != PlanetMode::Earth
        {
            return Err(invalid("unsupported planet generator revision"));
        }
        if !(318_550_000..=EARTH_RADIUS_MM).contains(&self.radius_mm) {
            return Err(invalid("planet radius must be 0.05..=1 Earth radii"));
        }
        if !(1..=1000).contains(&self.height_scale_permille) {
            return Err(invalid("height scale must be 1..=1000 permille"));
        }
        if self.mountain_limit_mm > 5_000_000 {
            return Err(invalid("mountain limit cannot exceed 5 km"));
        }
        if self.sea_level_mm > i64::from(self.mountain_limit_mm) {
            return Err(invalid("sea level cannot exceed mountain limit"));
        }
        if self.detail_wavelength_mm == 0 || self.mountain_band_frequency == 0 {
            return Err(invalid(
                "detail wavelength and band frequency must be positive",
            ));
        }
        if self.detail_strength_mm > 1_000_000
            || self.detail_wavelength_mm > 1_000_000_000
            || self.mountain_band_frequency > 1024
            || !(-20_000_000..=20_000_000).contains(&self.sea_level_mm)
        {
            return Err(invalid(
                "planet terrain parameter exceeds its supported range",
            ));
        }
        if self.mode == PlanetMode::Procedural && self.earth_package.is_some() {
            return Err(invalid("earth_package is only valid in Earth mode"));
        }
        Ok(())
    }

    pub fn definition(&self) -> minimium_core::world::PlanetDefinition {
        minimium_core::world::PlanetDefinition {
            mode: match self.mode {
                PlanetMode::Procedural => 0,
                PlanetMode::Earth => 1,
            },
            radius_mm: self.radius_mm,
            height_scale_permille: self.height_scale_permille,
            sea_level_mm: self.sea_level_mm,
            mountain_limit_mm: self.mountain_limit_mm,
            detail_strength_mm: self.detail_strength_mm,
            detail_wavelength_mm: self.detail_wavelength_mm,
            mountain_band_frequency: self.mountain_band_frequency,
            package_digest: self.package_digest,
        }
    }
}

pub fn load(path: &Path, seed: u128) -> io::Result<ResolvedPlanetConfig> {
    let mut active = HashSet::new();
    let preset = load_preset(path, 0, &mut active)?;
    ResolvedPlanetConfig::from_preset(preset, seed)
}

fn load_preset(path: &Path, depth: usize, active: &mut HashSet<PathBuf>) -> io::Result<Preset> {
    if depth > MAX_EXTENDS_DEPTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "planet preset inheritance is too deep",
        ));
    }
    let path = path.canonicalize()?;
    if !active.insert(path.clone()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "planet preset inheritance cycle",
        ));
    }
    let source = fs::read_to_string(&path)?;
    let child: Preset = ron::from_str(&source).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {error}", path.display()),
        )
    })?;
    let mut result = if let Some(parent) = &child.extends {
        load_preset(
            &path
                .parent()
                .expect("canonical file has parent")
                .join(parent),
            depth + 1,
            active,
        )?
    } else {
        Preset::default()
    };
    result.overlay(child);
    active.remove(&path);
    Ok(result)
}
