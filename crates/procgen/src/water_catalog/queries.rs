use super::{dot, WaterCatalog};
use minimium_world::fluid::WaterBodyId;
use std::collections::BTreeSet;

impl WaterCatalog {
    /// Returns every catalog body whose sampled or authored footprint may touch
    /// an angular cap. The face-grid padding deliberately exceeds the maximum
    /// cube-face nearest-sample distance, avoiding false negatives at seams.
    pub fn body_ids_near_direction(
        &self,
        direction: [f64; 3],
        angular_radius_radians: f64,
    ) -> Vec<WaterBodyId> {
        let length = direction
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        if !length.is_finite()
            || length <= f64::EPSILON
            || !angular_radius_radians.is_finite()
            || angular_radius_radians < 0.0
        {
            return Vec::new();
        }
        let center = direction.map(|value| value / length);
        let mut bodies = BTreeSet::new();
        if angular_radius_radians >= std::f64::consts::PI {
            bodies.extend(self.bodies.iter().map(|body| body.id));
            return bodies.into_iter().collect();
        }

        let resolution = usize::from(self.resolution);
        let half_cell_padding = 4.0 / f64::from(self.resolution.max(1));
        let search_angle = (angular_radius_radians + half_cell_padding).min(std::f64::consts::PI);
        let minimum_dot = search_angle.cos() - 1.0e-12;
        for (face_index, grid) in self.face_bodies.iter().enumerate() {
            for (index, id) in grid.iter().enumerate() {
                if id.0 == 0 {
                    continue;
                }
                let x = index % (resolution + 1);
                let y = index / (resolution + 1);
                let u = 2.0 * x as f64 / resolution as f64 - 1.0;
                let v = 2.0 * y as f64 / resolution as f64 - 1.0;
                let cube = match face_index {
                    0 => [1.0, u, v],
                    1 => [-1.0, u, v],
                    2 => [u, 1.0, v],
                    3 => [u, -1.0, v],
                    4 => [u, v, 1.0],
                    5 => [u, v, -1.0],
                    _ => continue,
                };
                let magnitude = cube.iter().map(|value| value * value).sum::<f64>().sqrt();
                let sample_direction = cube.map(|value| value / magnitude);
                if dot(center, sample_direction) >= minimum_dot {
                    bodies.insert(*id);
                }
            }
        }

        let planet_radius_m = self.terrain.radius_mm() as f64 / 1000.0;
        for lake in &self.explicit_lakes {
            let lake_angle = (lake.radius_m / planet_radius_m).clamp(0.0, std::f64::consts::PI);
            if dot(center, lake.center).clamp(-1.0, 1.0).acos()
                <= angular_radius_radians + lake_angle
            {
                bodies.insert(lake.body.id);
            }
        }
        bodies.into_iter().collect()
    }
}
