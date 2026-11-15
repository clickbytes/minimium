use minimium_fluids::{FluidSnapshot, TerrainFluidGeometry, CELL_SIZE_M};
use minimium_procgen::WaterCatalog;
use std::sync::Arc;

pub(super) fn active_cell_water_at_level(
    catalog: &WaterCatalog,
    body: minimium_world::fluid::WaterBodyId,
    cell: [i32; 3],
    capacity_m3: f64,
    level_m: f64,
) -> f64 {
    if !capacity_m3.is_finite() || capacity_m3 <= 0.0 || !level_m.is_finite() {
        return 0.0;
    }
    const SAMPLES: i32 = 6;
    let mut free = 0_u32;
    let mut wet = 0_u32;
    for x in 0..SAMPLES {
        for y in 0..SAMPLES {
            for z in 0..SAMPLES {
                let offset = [x, y, z]
                    .map(|index| (f64::from(index) + 0.5) * CELL_SIZE_M / f64::from(SAMPLES));
                let point =
                    std::array::from_fn(|axis| f64::from(cell[axis]) * CELL_SIZE_M + offset[axis]);
                let radius = point.iter().map(|value| value * value).sum::<f64>().sqrt();
                if radius <= f64::EPSILON {
                    continue;
                }
                let direction = point.map(|value| value / radius);
                if catalog.body_at_direction(direction) != Some(body) {
                    continue;
                }
                let Some(ground) = catalog.ground_radius_at_direction_m(direction) else {
                    continue;
                };
                if radius >= ground {
                    free += 1;
                    if radius <= level_m {
                        wet += 1;
                    }
                }
            }
        }
    }
    if free == 0 {
        0.0
    } else {
        capacity_m3 * f64::from(wet) / f64::from(free)
    }
}

pub(super) fn snapshots_quiet(before: &FluidSnapshot, after: &FluidSnapshot) -> bool {
    if before.cells().len() != after.cells().len() || before.reservoirs() != after.reservoirs() {
        return false;
    }
    before.cells().iter().all(|(key, old)| {
        let Some(new) = after.cells().get(key) else {
            return false;
        };
        (old.volume_m3 - new.volume_m3).abs() <= 1.0e-10
            && old
                .positive_velocity_m_s
                .iter()
                .zip(new.positive_velocity_m_s)
                .all(|(a, b)| (a - b).abs() <= 1.0e-6)
            && old
                .negative_boundary_velocity_m_s
                .iter()
                .zip(new.negative_boundary_velocity_m_s)
                .all(|(a, b)| (a - b).abs() <= 1.0e-6)
    })
}

pub(super) struct CatalogWaterRead {
    snapshot: Arc<FluidSnapshot>,
    catalog: Arc<WaterCatalog>,
}

impl CatalogWaterRead {
    pub(super) fn new(snapshot: Arc<FluidSnapshot>, catalog: Arc<WaterCatalog>) -> Self {
        Self { snapshot, catalog }
    }
}

impl minimium_world::fluid::WaterRead for CatalogWaterRead {
    fn sample_capsule(
        &self,
        feet: [f64; 3],
        up: [f64; 3],
        height: f64,
    ) -> minimium_world::fluid::WaterSample {
        if !height.is_finite()
            || height <= 0.0
            || feet.iter().chain(up.iter()).any(|v| !v.is_finite())
        {
            return minimium_world::fluid::WaterSample::default();
        }
        let mut wet = 0.0;
        let mut flow = [0.0; 3];
        for index in 0..5 {
            let t = index as f64 / 4.0;
            let point: [f64; 3] = std::array::from_fn(|axis| feet[axis] + up[axis] * height * t);
            let key = point.map(|v| (v / CELL_SIZE_M).floor() as i32);
            if let Some(sample) = self.snapshot.sample_cell(key) {
                wet += sample.submerged_fraction;
                for axis in 0..3 {
                    flow[axis] += sample.velocity_m_s[axis];
                }
                continue;
            }
            let radius = point.iter().map(|v| v * v).sum::<f64>().sqrt();
            if radius <= f64::EPSILON {
                continue;
            }
            let direction = point.map(|v| v / radius);
            let Some(id) = self
                .catalog
                .body_at_direction(direction)
                .filter(|id| id.0 != 0)
            else {
                continue;
            };
            let Some(ground) = self.catalog.ground_radius_at_direction_m(direction) else {
                continue;
            };
            let Some(reservoir) = self.snapshot.reservoirs().get(&id) else {
                continue;
            };
            if radius >= ground && radius <= reservoir.level_m().unwrap_or(f64::NEG_INFINITY) {
                wet += 1.0;
            }
        }
        minimium_world::fluid::WaterSample {
            submerged_fraction: (wet / 5.0).clamp(0.0, 1.0),
            velocity_m_s: flow.map(|v| v / 5.0),
        }
    }
}

pub(super) struct CatalogWaterGeometry<'a> {
    pub(super) terrain: &'a TerrainFluidGeometry<'a>,
    pub(super) catalog: Option<&'a WaterCatalog>,
    pub(super) snapshot: &'a FluidSnapshot,
    pub(super) center_m: [f64; 3],
}

impl minimium_fluids::FluidGeometry for CatalogWaterGeometry<'_> {
    fn capacity_m3(&self, cell: [i32; 3]) -> Option<f64> {
        self.terrain.capacity_m3(cell)
    }

    fn face(
        &self,
        cell: [i32; 3],
        axis: usize,
        positive: bool,
    ) -> Option<minimium_fluids::FluidFace> {
        let mut face = self.terrain.face(cell, axis, positive)?;
        if face.open_area_m2 <= 0.0 {
            return Some(face);
        }
        if let minimium_fluids::WaterEndpoint::Cell(neighbor) = face.boundary {
            if !self.snapshot.cells().contains_key(&neighbor) {
                let mut endpoint = minimium_fluids::WaterEndpoint::Air;
                if let Some(catalog) = self.catalog {
                    let mut point = cell.map(|v| f64::from(v) * CELL_SIZE_M + CELL_SIZE_M * 0.5);
                    point[axis] += if positive {
                        CELL_SIZE_M * 0.5
                    } else {
                        -CELL_SIZE_M * 0.5
                    };
                    let radial: [f64; 3] =
                        std::array::from_fn(|axis| point[axis] - self.center_m[axis]);
                    let radius = radial.iter().map(|v| v * v).sum::<f64>().sqrt();
                    if radius > f64::EPSILON {
                        let direction = radial.map(|v| v / radius);
                        if let (Some(id), Some(ground_radius)) = (
                            catalog.body_at_direction(direction),
                            catalog.ground_radius_at_direction_m(direction),
                        ) {
                            if let Some(reservoir) = self.snapshot.reservoirs().get(&id) {
                                if radius >= ground_radius && radius <= reservoir.level_m().ok()? {
                                    endpoint = minimium_fluids::WaterEndpoint::Reservoir(id);
                                }
                            }
                        }
                    }
                }
                face.boundary = endpoint;
            }
        }
        Some(face)
    }
}
