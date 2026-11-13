//! Shader layouts for weather-cloud frame and instance data.

/// Projection, observer and cloud-window controls for one cloud draw.
#[repr(C, align(16))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CloudUniform {
    pub projection: [[f32; 4]; 4],
    pub scene_projection: [[f32; 4]; 4],
    pub inverse_projection: [[f32; 4]; 4],
    pub origin_radius: [f32; 4],
    pub eye: [f32; 4],
    pub observer: [f32; 4],
    pub config: [u32; 4],
    pub inside: [[u32; 4]; 4],
}

/// Per-cloud center, age, layer and interpolation data.
#[repr(C, align(16))]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CloudRecord {
    pub center_age: [f32; 4],
    pub east_opacity: [f32; 4],
    pub up_layer: [f32; 4],
    pub north_region: [f32; 4],
    pub morph: [f32; 4],
    pub half_size: [f32; 4],
}

impl CloudUniform {
    /// Select at most sixteen containing ellipsoids, once per frame, not per pixel.
    /// Snapshot order is stable; excess overlaps use the surface mask only.
    pub fn select_inside(&mut self, records: &[CloudRecord], enabled: bool) {
        self.config[3] = 0;
        self.inside = [[0; 4]; 4];
        if !enabled {
            return;
        }
        let eye = glam::Vec3::from_slice(&self.eye);
        for (index, cloud) in records.iter().enumerate() {
            let half = glam::Vec3::from_slice(&cloud.half_size).max(glam::Vec3::ONE);
            let up = glam::Vec3::from_slice(&cloud.up_layer);
            let delta = eye - glam::Vec3::from_slice(&cloud.center_age) - up * half.y;
            let local = glam::Vec3::new(
                delta.dot(glam::Vec3::from_slice(&cloud.east_opacity)),
                delta.dot(up),
                delta.dot(glam::Vec3::from_slice(&cloud.north_region)),
            ) / half;
            if local.length_squared() < 1.0 {
                let slot = self.config[3] as usize;
                self.inside[slot / 4][slot % 4] = index as u32;
                self.config[3] += 1;
                if self.config[3] == 16 {
                    break;
                }
            }
        }
    }
}

const _: () = {
    assert!(std::mem::size_of::<CloudUniform>() == 320);
    assert!(std::mem::size_of::<CloudRecord>() == 96);
    assert!(std::mem::align_of::<CloudUniform>() == 16);
    assert!(std::mem::offset_of!(CloudUniform, origin_radius) == 192);
    assert!(std::mem::offset_of!(CloudRecord, north_region) == 48);
    assert!(std::mem::offset_of!(CloudRecord, morph) == 64);
    assert!(std::mem::offset_of!(CloudRecord, half_size) == 80);
    assert!(std::mem::offset_of!(CloudUniform, inside) == 256);
};
