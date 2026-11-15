use super::*;

impl minimium_world::fluid::WaterRead for FluidSnapshot {
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
        let mut fraction = 0.0;
        let mut velocity = [0.0; 3];
        let mut found = 0.0;
        // Five deterministic capsule samples give a bounded shared observation.
        for index in 0..5 {
            let t = index as f64 / 4.0;
            let point: [f64; 3] = std::array::from_fn(|axis| feet[axis] + up[axis] * height * t);
            let key = point.map(|v| (v / CELL_SIZE_M).floor() as i32);
            if let Some(sample) = self.sample_cell(key) {
                fraction += sample.submerged_fraction;
                found += 1.0;
                for axis in 0..3 {
                    velocity[axis] += sample.velocity_m_s[axis];
                }
            }
        }
        if found > 0.0 {
            for value in &mut velocity {
                *value /= found;
            }
            minimium_world::fluid::WaterSample {
                submerged_fraction: (fraction / 5.0).clamp(0.0, 1.0),
                velocity_m_s: velocity,
            }
        } else {
            minimium_world::fluid::WaterSample::default()
        }
    }
}
