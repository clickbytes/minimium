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
}

const _: () = {
    assert!(std::mem::size_of::<CloudUniform>() == 256);
    assert!(std::mem::size_of::<CloudRecord>() == 80);
    assert!(std::mem::align_of::<CloudUniform>() == 16);
    assert!(std::mem::offset_of!(CloudUniform, origin_radius) == 192);
    assert!(std::mem::offset_of!(CloudRecord, north_region) == 48);
    assert!(std::mem::offset_of!(CloudRecord, morph) == 64);
};
