//! Committed crater height fields; object/remnant state is owned separately.
use minimium_world::terrain_edits::{project, CraterStamp, MAX_TERRAIN_EDITS};
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
}

fn cell(p: [f64; 3]) -> [i32; 3] {
    p.map(|v| (v / CELL_M).floor() as i32)
}
