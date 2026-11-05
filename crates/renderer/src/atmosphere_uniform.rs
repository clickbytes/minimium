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
}

const _: () = {
    assert!(std::mem::size_of::<AtmosphereUniform>() == 80);
    assert!(std::mem::align_of::<AtmosphereUniform>() == 16);
};
