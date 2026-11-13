//! Bounded light-path proxies from the existing weather publication. No meshing.
use crate::{
    cloud_uniform::CloudRecord,
    transient_lighting::{CloudLightOccluder, TransientLightUniform, OCCLUDERS_PER_LIGHT},
};
use glam::Vec3;

fn smooth(a: f32, b: f32, value: f32) -> f32 {
    let x = ((value - a) / (b - a)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

impl TransientLightUniform {
    /// One bounded scan of the published local snapshot per selected light.
    /// Strongest relevant optical depths win, ties retain publication order.
    pub fn attach_clouds(&mut self, records: &[CloudRecord], eye: [f32; 3], standard: bool) {
        self.clouds.fill(CloudLightOccluder::default());
        let limit = if standard { OCCLUDERS_PER_LIGHT } else { 4 };
        let eye = Vec3::from_array(eye);
        for light_index in 0..self.count[0] as usize {
            let source = &mut self.sources[light_index];
            source.range_lifetime[2] = 0.0;
            let end = Vec3::from_slice(&source.position_radius);
            let delta = end - eye;
            let mut scores = [0.0; OCCLUDERS_PER_LIGHT];
            for (cloud_index, cloud) in records.iter().enumerate() {
                let half = Vec3::from_slice(&cloud.half_size).max(Vec3::ONE);
                let up = Vec3::from_slice(&cloud.up_layer);
                let center = Vec3::from_slice(&cloud.center_age) + up * half.y;
                let t =
                    ((center - eye).dot(delta) / delta.length_squared().max(1.0)).clamp(0.0, 1.0);
                let separation = center.distance(eye + delta * t);
                // Cover nearby receivers as well as the observer ray. Far-field
                // receivers beyond this 10-km corridor use incomplete proxies.
                let support = half.max_element() + 10_000.0;
                if separation > support {
                    continue;
                }
                let age = cloud.center_age[3];
                let fade = smooth(0.0, 420.0, age) * (1.0 - smooth(2800.0, 3600.0, age));
                let tau = cloud.east_opacity[3] * fade;
                let score = tau / (1.0 + separation / half.max_element());
                let Some(slot) = scores[..limit].iter().position(|old| score > *old) else {
                    continue;
                };
                let base = light_index * OCCLUDERS_PER_LIGHT;
                for i in (slot + 1..limit).rev() {
                    scores[i] = scores[i - 1];
                    self.clouds[base + i] = self.clouds[base + i - 1];
                }
                scores[slot] = score;
                self.clouds[base + slot] = CloudLightOccluder {
                    center_extinction: center.extend(tau / (2.0 * half.y)).to_array(),
                    east_inverse: (Vec3::from_slice(&cloud.east_opacity) / half.x)
                        .extend((cloud_index + 1) as f32)
                        .to_array(),
                    up_inverse: (up / half.y).extend(0.0).to_array(),
                    north_inverse: (Vec3::from_slice(&cloud.north_region) / half.z)
                        .extend(0.0)
                        .to_array(),
                };
            }
            source.range_lifetime[2] =
                scores[..limit].iter().filter(|score| **score > 0.0).count() as f32;
        }
    }
}
