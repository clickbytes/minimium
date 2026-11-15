//! Read-only predictive water view installed from coherent excavation snapshots.
use minimium_fluids::FluidSnapshot;
use minimium_world::fluid::{WaterRead, WaterSample};
use std::sync::Arc;

#[derive(Default, Clone)]
pub struct WaterReplica {
    snapshot: Option<Arc<FluidSnapshot>>,
    catalog: Option<Arc<minimium_procgen::WaterCatalog>>,
    region: Option<([i32; 3], [i32; 3])>,
}

impl WaterReplica {
    pub fn new(snapshot: Arc<FluidSnapshot>) -> Self {
        Self {
            snapshot: Some(snapshot),
            catalog: None,
            region: None,
        }
    }

    pub fn install(&mut self, snapshot: Arc<FluidSnapshot>) {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|current| current.world() != snapshot.world())
        {
            self.catalog = None;
            self.region = None;
        }
        if self.snapshot.as_ref().is_none_or(|current| {
            current.world() != snapshot.world() || current.revision() <= snapshot.revision()
        }) {
            self.snapshot = Some(snapshot);
        }
    }

    pub fn clear(&mut self) {
        self.snapshot = None;
        self.catalog = None;
        self.region = None;
    }

    pub fn install_catalog(&mut self, catalog: Arc<minimium_procgen::WaterCatalog>) {
        self.catalog = Some(catalog);
    }

    pub fn install_region(&mut self, region: Option<([i32; 3], [i32; 3])>) {
        self.region = region;
    }

    pub fn snapshot(&self) -> Option<Arc<FluidSnapshot>> {
        self.snapshot.as_ref().map(Arc::clone)
    }

    pub fn as_read(&self) -> Option<Arc<dyn WaterRead>> {
        let snapshot = Arc::clone(self.snapshot.as_ref()?);
        let inner: Arc<dyn WaterRead> = match &self.catalog {
            Some(catalog) => Arc::new(CatalogReplicaRead {
                snapshot,
                catalog: Arc::clone(catalog),
            }),
            None => snapshot,
        };
        Some(Arc::new(RegionRead {
            inner,
            region: self.region,
        }))
    }

    pub fn sample(&self, feet_m: [f64; 3], up: [f64; 3], height_m: f64) -> WaterSample {
        self.as_read().map_or_else(WaterSample::default, |read| {
            read.sample_capsule(feet_m, up, height_m)
        })
    }
}

struct RegionRead {
    inner: Arc<dyn WaterRead>,
    region: Option<([i32; 3], [i32; 3])>,
}

impl WaterRead for RegionRead {
    fn sample_capsule(&self, feet: [f64; 3], up: [f64; 3], height: f64) -> WaterSample {
        let Some((minimum, maximum)) = self.region else {
            return WaterSample::default();
        };
        if !height.is_finite()
            || height <= 0.0
            || feet.iter().chain(up.iter()).any(|value| !value.is_finite())
        {
            return WaterSample::default();
        }
        let covered = (0..5).all(|sample| {
            let t = sample as f64 / 4.0;
            (0..3).all(|axis| {
                let cell = ((feet[axis] + up[axis] * height * t) / minimium_fluids::CELL_SIZE_M)
                    .floor() as i32;
                minimum[axis] <= cell && cell < maximum[axis]
            })
        });
        if covered {
            self.inner.sample_capsule(feet, up, height)
        } else {
            WaterSample::default()
        }
    }
}

struct CatalogReplicaRead {
    snapshot: Arc<FluidSnapshot>,
    catalog: Arc<minimium_procgen::WaterCatalog>,
}

impl WaterRead for CatalogReplicaRead {
    fn sample_capsule(&self, feet: [f64; 3], up: [f64; 3], height: f64) -> WaterSample {
        if !height.is_finite()
            || height <= 0.0
            || feet.iter().chain(up.iter()).any(|value| !value.is_finite())
        {
            return WaterSample::default();
        }
        let mut wet = 0.0;
        let mut flow = [0.0; 3];
        for index in 0..5 {
            let t = index as f64 / 4.0;
            let point: [f64; 3] = std::array::from_fn(|axis| feet[axis] + up[axis] * height * t);
            let key = point.map(|value| (value / minimium_fluids::CELL_SIZE_M).floor() as i32);
            if let Some(sample) = self.snapshot.sample_cell(key) {
                wet += sample.submerged_fraction;
                for axis in 0..3 {
                    flow[axis] += sample.velocity_m_s[axis];
                }
                continue;
            }
            let radius = point.iter().map(|value| value * value).sum::<f64>().sqrt();
            if radius <= f64::EPSILON {
                continue;
            }
            let direction = point.map(|value| value / radius);
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
        WaterSample {
            submerged_fraction: (wet / 5.0).clamp(0.0, 1.0),
            velocity_m_s: flow.map(|value| value / 5.0),
        }
    }
}
