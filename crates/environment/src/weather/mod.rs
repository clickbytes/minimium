//! Deterministic regional weather. No camera, GPU, network or persistence state.
mod climate;
mod families;
mod population;
mod shape;
pub use families::CloudKind;
pub use shape::DensityField;
pub mod region;

use glam::DVec3;
use minimium_world::world::{WorldManifest, WorldTopology};
use std::collections::HashMap;

pub const EARTH_RADIUS: f64 = 6_371_000.0;
pub const MAX_CLOUDS: usize = 8192;
pub const MAX_CACHED_CLOUDS: usize = 16384;
pub const CORE_ANGLE: f64 = 0.034;
pub const OUTER_ANGLE: f64 = 0.055;
pub(crate) const LIFETIME: f64 = 3600.0;
pub(crate) const POPULATION_SECONDS: f64 = 1800.0;
pub(crate) const WIND: [f64; 3] = [5.0, 10.0, 20.0];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeasonalProfile {
    AnnualMean,
    Seasonal,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum CloudDebug {
    #[default]
    Off,
    Regions,
    Layers,
    Wind,
    Kinds,
    Detail,
}

#[derive(Clone, Copy, Debug)]
pub struct LayerConditions {
    pub coverage: f64,
    pub base_m: f64,
    pub thickness_m: f64,
    pub max_base_m: f64,
    pub ceiling_m: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Conditions {
    pub temperature_c: f64,
    pub humidity: f64,
    pub water_supply: f64,
    pub uplift: f64,
    pub layers: [LayerConditions; 3],
}

#[derive(Clone, Copy, Debug)]
pub struct Cloud {
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

pub struct CloudSnapshot {
    pub utc: f64,
    pub revision: u64,
    pub observer: DVec3,
    pub conditions: Conditions,
    pub region: region::Region,
    pub regions: Vec<region::Region>,
    pub clouds: Vec<Cloud>,
    pub limited: bool,
}

pub struct WeatherModel<T: TerrainSampler> {
    pub manifest: WorldManifest,
    pub radius: f64,
    pub scale: f64,
    elevation_scale: f64,
    pub profile: SeasonalProfile,
    scenario: crate::climate::ClimateScenario,
    terrain: T,
    is_earth: bool,
    cache: HashMap<u64, Cloud>,
}

pub trait TerrainSampler: Send + Sync + 'static {
    fn sample(&self, direction: DVec3) -> (f64, bool);
    /// Source height above the sea datum, before any relief exaggeration.
    fn source_elevation_m(&self, _direction: DVec3) -> Option<f64> {
        None
    }
    fn contains_lake(&self, latitude_degrees: f64, longitude_degrees: f64) -> bool;
    fn earth_elevation_upper_bound_m(&self, direction: DVec3, angle: f64) -> Option<f64>;
    fn relief_bound_m(&self) -> f64;
}

impl<T: TerrainSampler> WeatherModel<T> {
    pub fn new(
        manifest: WorldManifest,
        profile: SeasonalProfile,
        radius_m: f64,
        is_earth: bool,
        terrain: T,
    ) -> Option<Self> {
        if manifest.topology() != WorldTopology::Planet {
            return None;
        }
        if !radius_m.is_finite() || radius_m <= 0.0 {
            return None;
        }
        Some(Self {
            manifest,
            radius: radius_m,
            scale: radius_m / EARTH_RADIUS,
            elevation_scale: manifest
                .planet()
                .filter(|planet| planet.mode == 1)
                .map_or(1.0, |planet| {
                    f64::from(planet.height_scale_permille.max(1)) / 1000.0
                }),
            profile,
            scenario: crate::climate::ClimateScenario::default(),
            terrain,
            is_earth,
            cache: HashMap::with_capacity(MAX_CACHED_CLOUDS),
        })
    }

    pub fn sample(&self, direction: DVec3, utc: f64) -> Conditions {
        climate::sample(self, direction.normalize(), utc)
    }

    pub fn set_scenario(
        &mut self,
        scenario: crate::climate::ClimateScenario,
    ) -> Result<(), &'static str> {
        scenario.validate()?;
        if self.scenario == scenario {
            return Ok(());
        }
        self.scenario = scenario;
        self.cache.clear();
        Ok(())
    }

    pub fn surface_cover(&self, direction: DVec3, utc: f64) -> crate::climate::SurfaceCoverSample {
        self.surface_state(direction, utc).cover
    }

    pub fn surface_state(&self, direction: DVec3, utc: f64) -> crate::climate::SurfaceState {
        let direction = direction.normalize();
        let site = climate::site(self, direction);
        let at = |time| {
            let (temperature_c, humidity, _) = climate::climate_at(self, direction, time, &site);
            crate::climate::ClimateSample {
                temperature_c,
                moisture: humidity,
                seasonality: crate::climate::seasonality(direction, time),
            }
        };
        crate::climate::SurfaceState {
            cover: crate::climate::surface_cover(utc, at),
            climate: at(utc),
            source_elevation_m: self.ground_elevation_m(direction),
            climate_elevation_m: self.climate_elevation_m(direction),
        }
    }

    /// Physical source elevation for presentation weather, without GPU or I/O.
    pub fn ground_elevation_m(&self, direction: DVec3) -> f64 {
        if let Some(p) = self.manifest.planet().filter(|p| p.peak_relief.enabled()) {
            let direction = direction.normalize();
            return self
                .terrain
                .source_elevation_m(direction)
                .map(|h| h.max(0.0))
                .unwrap_or_else(|| p.source_height_above_sea_m(self.terrain(direction).0));
        }
        self.terrain(direction.normalize()).0 / self.elevation_scale
    }

    /// Rendered ground expressed in the linear optical atmosphere coordinate scale.
    pub fn atmospheric_ground_height_m(&self, direction: DVec3) -> f64 {
        self.terrain(direction.normalize()).0 / self.elevation_scale
    }

    pub fn climate_elevation_m(&self, direction: DVec3) -> f64 {
        let source = self.ground_elevation_m(direction);
        let influence = self.scenario.relief_influence;
        if influence == 0.0 {
            source
        } else {
            source + (self.atmospheric_ground_height_m(direction) - source) * influence
        }
    }

    fn seed(&self) -> u64 {
        let seed = self.manifest.generation().seed().get();
        hash(seed as u64) ^ hash((seed >> 64) as u64).rotate_left(17)
    }

    pub fn terrain(&self, direction: DVec3) -> (f64, bool) {
        self.terrain.sample(direction)
    }

    pub fn contains_lake(&self, direction: DVec3) -> bool {
        self.terrain.contains_lake(
            direction.y.asin().to_degrees(),
            direction.z.atan2(direction.x).to_degrees(),
        )
    }

    pub const fn is_earth(&self) -> bool {
        self.is_earth
    }

    pub fn query(&mut self, observer: DVec3, utc: f64, revision: u64) -> CloudSnapshot {
        population::query(self, observer.normalize(), utc, revision)
    }
}

pub fn surface_key(direction: DVec3) -> minimium_world::world::CubeSampleKey {
    region::surface_key(direction)
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
