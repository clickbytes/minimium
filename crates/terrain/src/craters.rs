//! Committed crater height fields; object/remnant state is owned separately.
use minimium_world::terrain_edits::{length, project, CraterStamp, MAX_TERRAIN_EDITS};
use std::collections::BTreeMap;
const CELL_M: f64 = 512.0;

#[derive(Clone, Debug)]
pub struct TerrainSnapshot {
    radius_m: f64,
    edits: Vec<CraterStamp>,
    cells: BTreeMap<[i32; 3], Vec<usize>>,
    revision: u64,
}

impl TerrainSnapshot {
    pub fn sector_revision(
        &self,
        key: minimium_world::world::SectorKey,
        span: u16,
        count: u16,
    ) -> u64 {
        if count == 0 {
            return 0;
        }
        let minimium_world::world::SectorKey::Planet(key) = key else {
            return 0;
        };
        let (u, v) = key.coordinates();
        let scale = minimium_world::world::PLANET_SAMPLE_COORDINATE_SCALE as i64;
        let coordinate = |p: u16| {
            (-scale + scale * (2 * i64::from(p) + i64::from(span)) / i64::from(count)) as i32
        };
        key.face()
            .cube_sample(coordinate(u), coordinate(v))
            .map_or(0, |sample| {
                self.patch_revision(
                    sample.coordinates().map(f64::from),
                    self.radius_m * 4.0 * f64::from(span) / f64::from(count),
                )
            })
    }
    pub fn empty(radius_m: f64) -> Self {
        Self {
            radius_m,
            edits: Vec::new(),
            cells: BTreeMap::new(),
            revision: 0,
        }
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn edits(&self) -> &[CraterStamp] {
        &self.edits
    }
    pub fn radius_m(&self) -> f64 {
        self.radius_m
    }
    pub fn insert(&mut self, stamp: CraterStamp) -> Result<bool, &'static str> {
        if !stamp.valid() || !self.radius_m.is_finite() || self.radius_m < 1.0 {
            return Err("invalid terrain edit");
        }
        if let Some(existing) = self.edits.iter().find(|e| e.id == stamp.id) {
            return if *existing == stamp {
                Ok(false)
            } else {
                Err("conflicting impact ID")
            };
        }
        if self.edits.len() >= MAX_TERRAIN_EDITS {
            return Err("terrain capacity or revision exhausted");
        }
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("terrain revision exhausted")?;
        let p = project(stamp.center, self.radius_m);
        let reach = stamp.radius_m * 2.0 + 1.0;
        let lo = cell(p.map(|v| v - reach));
        let hi = cell(p.map(|v| v + reach));
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    self.cells
                        .entry([x, y, z])
                        .or_default()
                        .push(self.edits.len());
                }
            }
        }
        self.revision = revision;
        self.edits.push(stamp);
        Ok(true)
    }

    pub fn sample(&self, direction: [f64; 3]) -> (f64, f64) {
        let mut displacement = 0.0;
        let mut exposed = 0.0_f64;
        if let Some(indices) = self.cells.get(&cell(project(direction, self.radius_m))) {
            for &index in indices {
                let (height, material) = self.edits[index].displacement(direction, self.radius_m);
                displacement += height;
                exposed = exposed.max(material);
            }
        }
        (displacement, exposed)
    }

    /// Revision of edits intersecting this spherical patch, including its border.
    pub fn patch_revision(&self, direction: [f64; 3], bound_m: f64) -> u64 {
        let p = project(direction, self.radius_m);
        self.candidates(p, bound_m)
            .into_iter()
            .map(|i| &self.edits[i])
            .filter(|e| {
                let c = project(e.center, self.radius_m);
                length(std::array::from_fn(|i| p[i] - c[i])) <= bound_m + e.radius_m * 2.0
            })
            .fold(0, |revision, edit| {
                revision ^ edit.id.wrapping_mul(0x9e3779b97f4a7c15)
            })
    }

    fn candidates(&self, center: [f64; 3], bound: f64) -> Vec<usize> {
        let lo = cell(center.map(|x| x - bound));
        let hi = cell(center.map(|x| x + bound));
        let volume = (0..3)
            .map(|i| i64::from(hi[i]) - i64::from(lo[i]) + 1)
            .fold(1_i64, i64::saturating_mul);
        if volume > 512 {
            return (0..self.edits.len()).collect();
        }
        let mut indices = Vec::new();
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    if let Some(cell) = self.cells.get(&[x, y, z]) {
                        indices.extend_from_slice(cell);
                    }
                }
            }
        }
        indices.sort_unstable();
        indices.dedup();
        indices
    }
}

fn cell(p: [f64; 3]) -> [i32; 3] {
    p.map(|v| (v / CELL_M).floor() as i32)
}
