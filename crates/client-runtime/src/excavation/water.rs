use super::*;

impl ExcavationReplica {
    pub fn fluids(&self) -> Option<Arc<minimium_fluids::FluidSnapshot>> {
        self.fluids.as_ref().map(Arc::clone)
    }

    pub fn fluid_region(&self) -> Option<([i32; 3], [i32; 3])> {
        self.fluid_region
    }

    pub fn published_fluid_sync_id(&self) -> Option<u64> {
        self.published_fluid_sync_id
    }

    pub fn fluid_context(&self) -> Option<WorldContext> {
        self.fluids.as_ref().and(self.context)
    }

    pub fn water_region_covers_capsule(
        &self,
        feet_m: [f64; 3],
        up: [f64; 3],
        height_m: f64,
    ) -> bool {
        if !height_m.is_finite()
            || height_m <= 0.0
            || feet_m
                .iter()
                .chain(up.iter())
                .any(|value| !value.is_finite())
        {
            return false;
        }
        let Some((minimum, maximum)) = self.fluid_region else {
            return false;
        };
        if self.fluids.is_none() {
            return false;
        }
        (0..5).all(|sample| {
            let t = sample as f64 / 4.0;
            (0..3).all(|axis| {
                let coordinate =
                    (feet_m[axis] + up[axis] * height_m * t) / minimium_fluids::CELL_SIZE_M;
                let cell = coordinate.floor() as i32;
                minimum[axis] <= cell && cell < maximum[axis]
            })
        })
    }

    pub fn craters(&self) -> Option<Arc<TerrainSnapshot>> {
        self.craters.as_ref().map(Arc::clone)
    }

    pub fn water_read(&self) -> Option<Arc<dyn minimium_world::fluid::WaterRead>> {
        self.fluids
            .as_ref()
            .map(|snapshot| Arc::clone(snapshot) as Arc<dyn minimium_world::fluid::WaterRead>)
    }
}
