//! Fixed uniform layout shared with the spherical atmosphere shaders.

/// Clear-air Earth atmosphere parameters uploaded to the renderer.
#[repr(C, align(16))]
#[derive(Clone, Copy, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct AtmosphereUniform {
    pub radii: [f32; 4],
    pub rayleigh: [f32; 4],
    pub mie: [f32; 4],
    pub ozone: [f32; 4],
    pub config: [f32; 4],
    pub up_height: [f32; 4],
    pub eye_relative: [f32; 4],
    pub weather: [f32; 4],
    pub fog_center: [f32; 4],
    pub fog_east: [f32; 4],
    pub fog_north: [f32; 4],
    pub fog_time: [f32; 4],
    pub inverse_view_projection: [[f32; 4]; 4],
}

const _: () = {
    assert!(std::mem::size_of::<AtmosphereUniform>() == 256);
    assert!(std::mem::align_of::<AtmosphereUniform>() == 16);
    assert!(std::mem::offset_of!(AtmosphereUniform, up_height) == 80);
    assert!(std::mem::offset_of!(AtmosphereUniform, fog_center) == 128);
    assert!(std::mem::offset_of!(AtmosphereUniform, fog_time) == 176);
    assert!(std::mem::offset_of!(AtmosphereUniform, inverse_view_projection) == 192);
};
