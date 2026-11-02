//! Deterministic regional weather. No camera, GPU, network or persistence state.
mod climate;
mod families;
mod population;
mod shape;
pub(crate) use families::CloudKind;
pub(crate) use shape::DensityField;
pub(crate) mod region;

use glam::DVec3;
use minimium_core::world::{WorldManifest, WorldTopology};
use std::collections::HashMap;

pub(crate) const EARTH_RADIUS: f64 = 6_371_000.0;
pub(crate) const MAX_CLOUDS: usize = 8192;
pub(crate) const MAX_CACHED_CLOUDS: usize = 16384;
pub(crate) const CORE_ANGLE: f64 = 0.034;
pub(crate) const OUTER_ANGLE: f64 = 0.055;
pub(crate) const LIFETIME: f64 = 3600.0;
pub(crate) const POPULATION_SECONDS: f64 = 1800.0;
pub(crate) const WIND: [f64; 3] = [5.0, 10.0, 20.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SeasonalProfile {
    AnnualMean,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub(crate) enum CloudDebug {
    #[default]
    Off,
    Regions,
    Layers,
    Wind,
    Kinds,
    Detail,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct LayerConditions {
    pub coverage: f64,
    pub base_m: f64,
    pub thickness_m: f64,
    pub max_base_m: f64,
    pub ceiling_m: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Conditions {
    pub temperature_c: f64,
    pub humidity: f64,
    pub water_supply: f64,
    pub uplift: f64,
    pub layers: [LayerConditions; 3],
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Cloud {
    pub id: u64,
    pub source: region::Region,
    pub layer: usize,
    pub birth_utc: f64,
    pub direction: DVec3,
    pub base_m: f64,
    pub kind: CloudKind,
    /// Bounds include all deformation; the lattice is nested at every level.
    pub size_m: DVec3,
    pub min_level: u8,
    pub max_level: u8,
    pub opacity: f32,
}

impl Cloud {
    pub fn angular_speed(self) -> f64 {
        WIND[self.layer] / EARTH_RADIUS / (1.0 - self.direction.y.powi(2)).sqrt().max(0.03)
    }

    pub fn direction_at(self, utc: f64) -> DVec3 {
        rotate(
            self.direction,
            self.angular_speed() * (utc - self.birth_utc),
        )
    }

    pub fn extent_m(self) -> f64 {
        self.size_m.x.hypot(self.size_m.z) * 0.5
    }

    pub fn grid(self, level: u8) -> ([usize; 3], f64) {
        let cell = self.size_m.max_element() / f64::from(1_u32 << level);
        (
            self.size_m
                .to_array()
                .map(|v| (v / cell).round().max(1.0) as usize),
            cell,
        )
    }

    /// Per-cloud windows avoid a simultaneous whole-sky mesh transition.
    pub fn morph_window(self, utc: f64) -> (i64, f64, f64) {
        let step = self.kind.morph_seconds();
        let phase = random(self.id ^ 0x37a4) * step;
        let index = ((utc - self.birth_utc + phase) / step).floor() as i64;
        (index, index as f64 * step - phase, step)
    }
}

pub(crate) struct CloudSnapshot {
    pub utc: f64,
    pub revision: u64,
    pub observer: DVec3,
    pub conditions: Conditions,
    pub region: region::Region,
    pub regions: Vec<region::Region>,
    pub clouds: Vec<Cloud>,
    pub limited: bool,
}

pub(crate) struct WeatherModel {
    pub manifest: WorldManifest,
    pub radius: f64,
    pub scale: f64,
    pub profile: SeasonalProfile,
    terrain: minimium_procgen::PlanetTerrain,
    cache: HashMap<u64, Cloud>,
}

impl WeatherModel {
    pub fn new(manifest: WorldManifest, profile: SeasonalProfile) -> Option<Self> {
        if manifest.topology() != WorldTopology::Planet {
            return None;
        }
        let surface = minimium_procgen::planet_surface_for(manifest);
        let mut terrain = minimium_procgen::PlanetTerrain::new(manifest.generation(), surface)?;
        if let Some(definition) = manifest.planet() {
            terrain = terrain.with_definition(definition);
        }
        let radius = manifest
            .planet()
            .map_or(surface.radius_mm(), |p| p.radius_mm) as f64
            / 1000.0;
        Some(Self {
            manifest,
            radius,
            scale: radius / EARTH_RADIUS,
            profile,
            terrain,
            cache: HashMap::with_capacity(MAX_CACHED_CLOUDS),
        })
    }

    pub fn sample(&self, direction: DVec3, utc: f64) -> Conditions {
        climate::sample(self, direction.normalize(), utc)
    }

    fn seed(&self) -> u64 {
        let seed = self.manifest.generation().seed().get();
        hash(seed as u64) ^ hash((seed >> 64) as u64).rotate_left(17)
    }

    pub fn terrain(&self, direction: DVec3) -> (f64, bool) {
        let key = region::surface_key(direction);
        let point = self.terrain.sample(key);
        (point.height_mm as f64 / 1000.0 - self.radius, point.ocean)
    }

    pub fn query(&mut self, observer: DVec3, utc: f64, revision: u64) -> CloudSnapshot {
        population::query(self, observer.normalize(), utc, revision)
    }
}

pub(crate) fn rotate(p: DVec3, angle: f64) -> DVec3 {
    let (s, c) = angle.rem_euclid(std::f64::consts::TAU).sin_cos();
    DVec3::new(c * p.x - s * p.z, p.y, s * p.x + c * p.z)
}

pub(crate) fn hash(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

pub(crate) fn random(seed: u64) -> f64 {
    (hash(seed) >> 11) as f64 / (1_u64 << 53) as f64
}
