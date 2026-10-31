use super::coordinates::{self, Cell, BASE_LEVEL};
use super::motion;
use crate::environment::weather::{transport::FlowFrame, wind::EARTH_RADIUS};
use glam::{DQuat, DVec3};
use std::collections::{HashMap, HashSet};

pub(super) const EXTENT_M: f64 = 70000.0;

pub(super) struct Root {
    pub(super) anchored: bool,
    pub(super) cell: Cell,
    pub(super) rotation: DQuat,
    pub(super) population: i64,
    pub(super) layer: u8,
}

impl Root {
    pub(super) fn intersects(&self, cell: Cell, up: DVec3, radius: f64) -> bool {
        let reach = cell.radius() + radius + EXTENT_M / EARTH_RADIUS;
        (self.rotation * cell.center()).dot(up) >= reach.min(std::f64::consts::PI).cos()
    }
}

pub(super) struct SpatialIndex {
    pub(super) roots: Vec<Root>,
    bins: HashMap<Cell, Vec<usize>>,
}

impl SpatialIndex {
    pub(super) fn new(frame: &FlowFrame) -> Self {
        let mut roots = Vec::new();
        let mut bins: HashMap<Cell, Vec<usize>> = HashMap::new();
        for face in 0..6 {
            for y in 0..(1 << BASE_LEVEL) {
                for x in 0..(1 << BASE_LEVEL) {
                    let cell = Cell {
                        face,
                        level: BASE_LEVEL,
                        x,
                        y,
                    };
                    for layer in 0..3 {
                        for cohort in 0..3 {
                            let rotation = motion::rotation(
                                frame,
                                cell.center(),
                                [1800.0, 5500.0, 11000.0][layer],
                                cohort,
                            );
                            for anchored in [false, true] {
                                if anchored && layer != 0 {
                                    continue;
                                }
                                let rotation = if anchored { DQuat::IDENTITY } else { rotation };
                                let center = rotation * cell.center();
                                let bound = cell.radius() + EXTENT_M / EARTH_RADIUS;
                                for bin in coordinates::cap(center, bound, BASE_LEVEL) {
                                    bins.entry(bin).or_default().push(roots.len());
                                }
                                roots.push(Root {
                                    anchored,
                                    cell,
                                    rotation,
                                    population: frame.cohorts[cohort].id,
                                    layer: layer as u8,
                                });
                            }
                        }
                    }
                }
            }
        }
        Self { roots, bins }
    }

    pub(super) fn query(&self, up: DVec3, radius: f64) -> Vec<usize> {
        let mut ids = HashSet::new();
        for cell in coordinates::cap(up, radius, BASE_LEVEL) {
            if let Some(roots) = self.bins.get(&cell) {
                ids.extend(
                    roots
                        .iter()
                        .copied()
                        .filter(|&i| self.roots[i].intersects(self.roots[i].cell, up, radius)),
                );
            }
        }
        let mut ids: Vec<_> = ids.into_iter().collect();
        ids.sort_unstable();
        ids
    }

    pub(super) fn bytes(&self) -> usize {
        self.roots.len() * std::mem::size_of::<Root>()
            + self
                .bins
                .values()
                .map(|v| v.capacity() * std::mem::size_of::<usize>())
                .sum::<usize>()
    }
}
