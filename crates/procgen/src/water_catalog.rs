//! Bounded, deterministic connected-water catalogue over canonical cube samples.
use crate::PlanetTerrain;
use minimium_world::fluid::WaterBodyId;
use minimium_world::world::{CubeSampleKey, PlanetFace, PLANET_SAMPLE_COORDINATE_SCALE};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};

mod queries;

pub const DEFAULT_WATER_CATALOG_RESOLUTION: u16 = 64;
pub const MAX_WATER_CATALOG_RESOLUTION: u16 = 256;
pub const CONNECTED_OCEAN_ID: WaterBodyId = WaterBodyId(u64::MAX);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaterCurvePoint {
    pub level_m: f64,
    pub volume_m3: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WaterBody {
    pub id: WaterBodyId,
    pub ocean: bool,
    pub initial_level_m: f64,
    pub initial_volume_m3: f64,
    pub curve: Vec<WaterCurvePoint>,
}

#[derive(Clone)]
pub struct WaterCatalog {
    pub resolution: u16,
    pub digest: [u8; 32],
    pub bodies: Vec<WaterBody>,
    face_bodies: Vec<Vec<WaterBodyId>>,
    terrain: std::sync::Arc<PlanetTerrain>,
    explicit_lakes: Vec<ExplicitLake>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WaterBodyDefinition {
    pub body: WaterBody,
    pub center_direction: [f64; 3],
    pub radius_m: f64,
}

#[derive(Clone)]
struct ExplicitLake {
    body: WaterBody,
    center: [f64; 3],
    radius_m: f64,
}

#[derive(Clone, Copy)]
struct Sample {
    key: CubeSampleKey,
    elevation_m: f64,
    wet: bool,
}

impl WaterCatalog {
    /// Builds connected wet components from terrain shared by collision and presentation.
    /// Resolution is clamped to a conservative 6 * 257² canonical samples.
    pub fn build(
        terrain: &PlanetTerrain,
        requested_resolution: u16,
    ) -> Result<Self, crate::GenerationError> {
        let resolution = requested_resolution.clamp(8, MAX_WATER_CATALOG_RESOLUTION);
        let n = i32::from(resolution);
        let scale = PLANET_SAMPLE_COORDINATE_SCALE;
        let mut samples = Vec::<Sample>::new();
        let mut ids = HashMap::<CubeSampleKey, usize>::new();
        let mut edges = Vec::<(usize, usize)>::new();
        let mut face_samples = Vec::<Vec<usize>>::new();
        for face in PlanetFace::ALL {
            let mut grid = vec![0usize; (n as usize + 1) * (n as usize + 1)];
            for y in 0..=n {
                for x in 0..=n {
                    let u = (-i64::from(scale)
                        + (2 * i64::from(scale) * i64::from(x) / i64::from(n)))
                        as i32;
                    let v = (-i64::from(scale)
                        + (2 * i64::from(scale) * i64::from(y) / i64::from(n)))
                        as i32;
                    let key = face.cube_sample(u, v).ok_or_else(|| {
                        crate::GenerationError::materialization("invalid water catalog cube sample")
                    })?;
                    let index = if let Some(index) = ids.get(&key) {
                        *index
                    } else {
                        let point = terrain.ground_sample(key)?;
                        let elevation = point.height_mm as f64;
                        let index = samples.len();
                        samples.push(Sample {
                            key,
                            elevation_m: elevation / 1000.0,
                            wet: point.ocean,
                        });
                        ids.insert(key, index);
                        index
                    };
                    grid[(y * (n + 1) + x) as usize] = index;
                }
            }
            for y in 0..=n {
                for x in 0..=n {
                    let i = grid[(y * (n + 1) + x) as usize];
                    if x < n {
                        edges.push((i, grid[(y * (n + 1) + x + 1) as usize]));
                    }
                    if y < n {
                        edges.push((i, grid[((y + 1) * (n + 1) + x) as usize]));
                    }
                }
            }
            face_samples.push(grid);
        }
        let mut parent: Vec<_> = (0..samples.len()).collect();
        for (a, b) in edges {
            if samples[a].wet && samples[b].wet {
                union(&mut parent, a, b);
            }
        }
        let mut components = BTreeMap::<usize, Vec<usize>>::new();
        for (i, sample) in samples.iter().enumerate() {
            if sample.wet {
                let root = find(&mut parent, i);
                components.entry(root).or_default().push(i);
            }
        }
        if components.is_empty() {
            return Ok(Self {
                resolution,
                digest: [0; 32],
                bodies: Vec::new(),
                face_bodies: vec![vec![WaterBodyId(0); ((n + 1) * (n + 1)) as usize]; 6],
                terrain: std::sync::Arc::new(terrain.clone()),
                explicit_lakes: Vec::new(),
            });
        }
        let area_per_sample =
            4.0 * std::f64::consts::PI * (terrain.radius_mm() as f64 / 1000.0).powi(2)
                / (6.0 * f64::from(resolution).powi(2));
        let sea_level_m = (terrain.radius_mm() as f64 + terrain.sea_level_mm() as f64) / 1000.0;
        let largest = components
            .iter()
            .max_by_key(|(_, nodes)| nodes.len())
            .map(|(root, _)| *root)
            .ok_or_else(|| {
                crate::GenerationError::materialization("water catalog has no largest component")
            })?;
        let mut bodies = Vec::new();
        let mut root_ids = HashMap::<usize, WaterBodyId>::new();
        for (root, nodes) in components {
            let ocean = root == largest;
            let mut elevations: Vec<f64> = nodes.iter().map(|i| samples[*i].elevation_m).collect();
            elevations.sort_by(f64::total_cmp);
            let mut prefix = Vec::with_capacity(elevations.len() + 1);
            prefix.push(0.0);
            for elevation in &elevations {
                prefix.push(prefix.last().copied().unwrap() + elevation);
            }
            let mut levels = elevations.clone();
            levels.push(sea_level_m);
            levels.sort_by(f64::total_cmp);
            levels.dedup_by(|a, b| (*a - *b).abs() < 0.001);
            if levels.len() > 4096 {
                let mut reduced: Vec<f64> = (0..4095)
                    .map(|i| {
                        let index = i * (levels.len() - 1) / 4094;
                        levels[index]
                    })
                    .collect();
                reduced.push(sea_level_m);
                reduced.sort_by(f64::total_cmp);
                reduced.dedup_by(|a, b| (*a - *b).abs() < 0.001);
                levels = reduced;
            }
            let mut curve = Vec::with_capacity(levels.len().min(4096));
            for level in levels {
                let count = elevations.partition_point(|elevation| *elevation < level);
                let volume = (count as f64 * level - prefix[count]) * area_per_sample;
                if curve
                    .last()
                    .is_none_or(|p: &WaterCurvePoint| volume > p.volume_m3)
                {
                    curve.push(WaterCurvePoint {
                        level_m: level,
                        volume_m3: volume,
                    });
                }
            }
            if curve.len() < 2 {
                let base = curve.first().copied().unwrap_or(WaterCurvePoint {
                    level_m: 0.0,
                    volume_m3: 0.0,
                });
                curve = vec![
                    WaterCurvePoint {
                        level_m: base.level_m,
                        volume_m3: 0.0,
                    },
                    WaterCurvePoint {
                        level_m: base.level_m + 0.001,
                        volume_m3: area_per_sample * 0.001,
                    },
                ];
            }
            let id = if ocean {
                CONNECTED_OCEAN_ID
            } else {
                body_id(&samples, &nodes)
            };
            let initial_count = elevations.partition_point(|elevation| *elevation < sea_level_m);
            let initial_volume_m3 =
                (initial_count as f64 * sea_level_m - prefix[initial_count]) * area_per_sample;
            root_ids.insert(root, id);
            bodies.push(WaterBody {
                id,
                ocean,
                initial_level_m: sea_level_m,
                initial_volume_m3,
                curve,
            });
        }
        bodies.sort_by_key(|body| body.id);
        if bodies.len() > 65_536 {
            return Err(crate::GenerationError::materialization(
                "water catalog exceeds body limit",
            ));
        }
        let face_bodies: Vec<Vec<WaterBodyId>> = face_samples
            .iter()
            .map(|grid| {
                grid.iter()
                    .map(|i| {
                        if samples[*i].wet {
                            root_ids
                                .get(&find(&mut parent, *i))
                                .copied()
                                .unwrap_or(WaterBodyId(0))
                        } else {
                            WaterBodyId(0)
                        }
                    })
                    .collect()
            })
            .collect();
        let mut digest = Sha256::new();
        digest.update(resolution.to_le_bytes());
        for sample in &samples {
            for coordinate in sample.key.coordinates() {
                digest.update(coordinate.to_le_bytes());
            }
            digest.update(sample.elevation_m.to_bits().to_le_bytes());
            digest.update([u8::from(sample.wet)]);
        }
        for body in &bodies {
            digest.update(body.id.0.to_le_bytes());
            digest.update([u8::from(body.ocean)]);
            digest.update(body.initial_level_m.to_bits().to_le_bytes());
            digest.update(body.initial_volume_m3.to_bits().to_le_bytes());
            for p in &body.curve {
                digest.update(p.level_m.to_bits().to_le_bytes());
                digest.update(p.volume_m3.to_bits().to_le_bytes());
            }
        }
        for face in &face_bodies {
            for id in face {
                digest.update(id.0.to_le_bytes());
            }
        }
        Ok(Self {
            resolution,
            digest: digest.finalize().into(),
            bodies,
            face_bodies,
            terrain: std::sync::Arc::new(terrain.clone()),
            explicit_lakes: Vec::new(),
        })
    }

    /// Adds authored finite basins without replacing the connected ocean.
    pub fn with_explicit_lakes(mut self, definitions: Vec<WaterBodyDefinition>) -> Option<Self> {
        if definitions.len() > 65_536usize.saturating_sub(self.bodies.len()) {
            return None;
        }
        let radius_planet = self.terrain.radius_mm() as f64 / 1000.0;
        let mut lakes = Vec::with_capacity(definitions.len());
        for definition in definitions {
            let body = definition.body;
            let length = definition
                .center_direction
                .iter()
                .map(|v| v * v)
                .sum::<f64>()
                .sqrt();
            let curve_level = curve_level_for_volume(&body.curve, body.initial_volume_m3);
            if body.ocean
                || body.id.0 == 0
                || body.id == CONNECTED_OCEAN_ID
                || !definition.radius_m.is_finite()
                || !(0.5..=100_000.0).contains(&definition.radius_m)
                || !definition.center_direction.iter().all(|v| v.is_finite())
                || length <= f64::EPSILON
                || body.curve.len() < 2
                || body.curve.len() > 4096
                || body.curve[0].volume_m3 != 0.0
                || body
                    .curve
                    .windows(2)
                    .any(|w| w[0].level_m >= w[1].level_m || w[0].volume_m3 >= w[1].volume_m3)
                || body
                    .curve
                    .iter()
                    .any(|p| !p.level_m.is_finite() || !p.volume_m3.is_finite())
                || !body.initial_level_m.is_finite()
                || !body.initial_volume_m3.is_finite()
                || !(0.0..=body.curve.last()?.volume_m3).contains(&body.initial_volume_m3)
                || curve_level.is_none_or(|level| (level - body.initial_level_m).abs() > 0.001)
                || self.bodies.iter().any(|existing| existing.id == body.id)
                || lakes
                    .iter()
                    .any(|existing: &ExplicitLake| existing.body.id == body.id)
            {
                return None;
            }
            let center = definition.center_direction.map(|v| v / length);
            for existing in &lakes {
                let angular_distance =
                    dot(center, existing.center).clamp(-1.0, 1.0).acos() * radius_planet;
                if angular_distance <= definition.radius_m + existing.radius_m {
                    return None;
                }
            }
            lakes.push(ExplicitLake {
                body,
                center,
                radius_m: definition.radius_m,
            });
        }
        for lake in &lakes {
            self.bodies.push(lake.body.clone());
        }
        self.bodies.sort_by_key(|body| body.id);
        self.explicit_lakes = lakes;
        let mut hash = Sha256::new();
        hash.update(self.digest);
        for lake in &self.explicit_lakes {
            hash.update(lake.body.id.0.to_le_bytes());
            for component in lake.center {
                hash.update(component.to_bits().to_le_bytes());
            }
            hash.update(lake.radius_m.to_bits().to_le_bytes());
            hash.update(lake.body.initial_level_m.to_bits().to_le_bytes());
            hash.update(lake.body.initial_volume_m3.to_bits().to_le_bytes());
            for point in &lake.body.curve {
                hash.update(point.level_m.to_bits().to_le_bytes());
                hash.update(point.volume_m3.to_bits().to_le_bytes());
            }
        }
        self.digest = hash.finalize().into();
        Some(self)
    }

    /// Undisplaced sea radius of the canonical ocean lattice, in world metres.
    pub fn reference_sea_radius_m(&self) -> f64 {
        (self.terrain.radius_mm() as i64 + self.terrain.sea_level_mm()).max(1) as f64 / 1000.0
    }

    /// Resolves a radial world direction to the connected body sampled on its cube face.
    pub fn body_at_direction(&self, direction: [f64; 3]) -> Option<WaterBodyId> {
        let length = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
        if length <= f64::EPSILON {
            return None;
        }
        let direction = direction.map(|v| v / length);
        let planet_radius = self.terrain.radius_mm() as f64 / 1000.0;
        if let Some(lake) = self.explicit_lakes.iter().find(|lake| {
            dot(direction, lake.center).clamp(-1.0, 1.0).acos() * planet_radius <= lake.radius_m
        }) {
            return Some(lake.body.id);
        }
        self.grid_index(direction)
            .and_then(|(face, index)| self.face_bodies.get(face)?.get(index).copied())
    }

    pub fn ground_radius_at_direction_m(&self, direction: [f64; 3]) -> Option<f64> {
        if direction.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let length = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
        if length <= f64::EPSILON {
            return None;
        }
        let point = self
            .terrain
            .ground_sample(crate::terrain_surface::key(direction.map(|v| v / length)))
            .ok()?;
        Some(point.height_mm as f64 / 1000.0)
    }

    fn grid_index(&self, direction: [f64; 3]) -> Option<(usize, usize)> {
        if direction.iter().any(|v| !v.is_finite()) || self.face_bodies.len() != 6 {
            return None;
        }
        let [x, y, z] = direction;
        let dominant = x.abs().max(y.abs()).max(z.abs());
        if dominant <= f64::EPSILON {
            return None;
        }
        let (face, u, v) = if x.abs() == dominant {
            (if x >= 0.0 { 0 } else { 1 }, y / dominant, z / dominant)
        } else if y.abs() == dominant {
            (if y >= 0.0 { 2 } else { 3 }, x / dominant, z / dominant)
        } else {
            (if z >= 0.0 { 4 } else { 5 }, x / dominant, y / dominant)
        };
        let n = usize::from(self.resolution);
        let gx = (((u + 1.0) * 0.5 * n as f64).round() as isize).clamp(0, n as isize) as usize;
        let gy = (((v + 1.0) * 0.5 * n as f64).round() as isize).clamp(0, n as isize) as usize;
        Some((face, gy * (n + 1) + gx))
    }
}

fn body_id(samples: &[Sample], nodes: &[usize]) -> WaterBodyId {
    let key = nodes.iter().map(|i| samples[*i].key).min().unwrap();
    let mut hash = Sha256::new();
    for value in key.coordinates() {
        hash.update(value.to_le_bytes());
    }
    let bytes: [u8; 32] = hash.finalize().into();
    let mut id = u64::from_le_bytes(bytes[..8].try_into().unwrap());
    if id == 0 || id == u64::MAX {
        id ^= 0x9e37_79b9_7f4a_7c15;
    }
    WaterBodyId(id)
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let ra = find(parent, a);
    let rb = find(parent, b);
    if ra != rb {
        let (low, high) = if ra < rb { (ra, rb) } else { (rb, ra) };
        parent[high] = low;
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn curve_level_for_volume(curve: &[WaterCurvePoint], volume_m3: f64) -> Option<f64> {
    if curve.len() < 2 || !volume_m3.is_finite() {
        return None;
    }
    let index = curve
        .partition_point(|point| point.volume_m3 < volume_m3)
        .clamp(1, curve.len() - 1);
    let (low, high) = (curve[index - 1], curve[index]);
    let fraction = (volume_m3 - low.volume_m3) / (high.volume_m3 - low.volume_m3);
    Some(low.level_m + fraction * (high.level_m - low.level_m))
}
