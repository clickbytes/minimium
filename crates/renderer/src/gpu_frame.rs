//! Stable camera uniform layout shared with the frame shader.

use crate::RenderViewFrame;

/// Maximum visual halo sources packed into the environment frame uniform.
pub const MAX_HALO_BODIES: usize = 8;

/// A celestial halo source encoded for the shared environment shader.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct HaloSource {
    pub direction_extent: [f32; 4],
    pub linear_rgb_visibility: [f32; 4],
    pub source_direction: [f32; 4],
    pub parameters: [f32; 4],
}

/// Sun, Moon, sky and halo data uploaded for one environment frame.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct EnvironmentUniform {
    pub sun_geometric_world: [f32; 4],
    pub sun_apparent_world: [f32; 4],
    pub moon_geometric_world: [f32; 4],
    pub moon_apparent_world: [f32; 4],
    pub moon_light_world_phase: [f32; 4],
    pub moon_pole_world: [f32; 4],
    pub local_sun_rgb_visibility: [f32; 4],
    pub local_moon_rgb_visibility: [f32; 4],
    pub global_sun_direction: [f32; 4],
    pub global_sun_rgb: [f32; 4],
    pub ambient_rgb_exposure: [f32; 4],
    pub sky_zenith_rgb_sun_altitude: [f32; 4],
    pub sky_horizon_rgb_star_visibility: [f32; 4],
    pub observer_up_horizon_cos: [f32; 4],
    pub flags: [u32; 4],
    pub eqj_to_world: [[f32; 4]; 4],
    pub moon_disk_radiance: [f32; 4],
    pub star_animation: [f32; 4],
    pub moon_orbit_world: [f32; 4],
    pub halo_count: [u32; 4],
    pub halo_sources: [HaloSource; MAX_HALO_BODIES],
}
use bytemuck::{Pod, Zeroable};

/// Camera values uploaded to the shared world-frame bind group.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct CameraUniform {
    pub view_projection: [[f32; 4]; 4],
    pub sky_view_projection: [[f32; 4]; 4],
    pub inverse_sky_view_projection: [[f32; 4]; 4],
    pub viewport: [f32; 4],
    pub eye_world: [f32; 4],
}

/// Converts an immutable camera frame to its shader uniform representation.
pub fn camera_uniform(view: RenderViewFrame) -> CameraUniform {
    CameraUniform {
        view_projection: view.view_projection,
        sky_view_projection: view.sky_view_projection,
        inverse_sky_view_projection: view.inverse_sky_view_projection,
        viewport: [
            view.viewport[0] as f32,
            view.viewport[1] as f32,
            1.0 / view.viewport[0].max(1) as f32,
            1.0 / view.viewport[1].max(1) as f32,
        ],
        eye_world: [
            view.camera_eye_world_m[0] as f32,
            view.camera_eye_world_m[1] as f32,
            view.camera_eye_world_m[2] as f32,
            0.0,
        ],
    }
}

/// Prepares dynamic visual flags and halo data for the environment uniform.
pub fn environment_uniform(
    mut uniform: EnvironmentUniform,
    view: RenderViewFrame,
    frame_available: bool,
    halo_count: u32,
    halo_sources: [HaloSource; MAX_HALO_BODIES],
) -> EnvironmentUniform {
    use std::sync::OnceLock;
    use std::time::Instant;

    static ANIMATION_EPOCH: OnceLock<Instant> = OnceLock::new();
    uniform.star_animation[0] = ANIMATION_EPOCH
        .get_or_init(Instant::now)
        .elapsed()
        .as_secs_f32();
    uniform.flags[1] = view.kind as u32;
    uniform.flags[2] = u32::from(frame_available);
    uniform.flags[3] = u32::from(!view.srgb_surface);
    uniform.halo_count = [halo_count.min(MAX_HALO_BODIES as u32), 0, 0, 0];
    uniform.halo_sources = halo_sources;
    uniform
}

const _: () = {
    assert!(std::mem::size_of::<CameraUniform>() == 224);
    assert!(std::mem::align_of::<CameraUniform>() == 16);
    assert!(std::mem::offset_of!(CameraUniform, view_projection) == 0);
    assert!(std::mem::offset_of!(CameraUniform, sky_view_projection) == 64);
    assert!(std::mem::offset_of!(CameraUniform, inverse_sky_view_projection) == 128);
    assert!(std::mem::offset_of!(CameraUniform, viewport) == 192);
    assert!(std::mem::offset_of!(CameraUniform, eye_world) == 208);
    assert!(std::mem::size_of::<HaloSource>() == 64);
    assert!(std::mem::align_of::<HaloSource>() == 16);
    assert!(std::mem::size_of::<EnvironmentUniform>() == 880);
    assert!(std::mem::align_of::<EnvironmentUniform>() == 16);
    assert!(std::mem::offset_of!(EnvironmentUniform, sun_geometric_world) == 0);
    assert!(std::mem::offset_of!(EnvironmentUniform, sun_apparent_world) == 16);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_geometric_world) == 32);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_apparent_world) == 48);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_light_world_phase) == 64);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_pole_world) == 80);
    assert!(std::mem::offset_of!(EnvironmentUniform, local_sun_rgb_visibility) == 96);
    assert!(std::mem::offset_of!(EnvironmentUniform, local_moon_rgb_visibility) == 112);
    assert!(std::mem::offset_of!(EnvironmentUniform, global_sun_direction) == 128);
    assert!(std::mem::offset_of!(EnvironmentUniform, global_sun_rgb) == 144);
    assert!(std::mem::offset_of!(EnvironmentUniform, ambient_rgb_exposure) == 160);
    assert!(std::mem::offset_of!(EnvironmentUniform, sky_zenith_rgb_sun_altitude) == 176);
    assert!(std::mem::offset_of!(EnvironmentUniform, sky_horizon_rgb_star_visibility) == 192);
    assert!(std::mem::offset_of!(EnvironmentUniform, observer_up_horizon_cos) == 208);
    assert!(std::mem::offset_of!(EnvironmentUniform, flags) == 224);
    assert!(std::mem::offset_of!(EnvironmentUniform, eqj_to_world) == 240);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_disk_radiance) == 304);
    assert!(std::mem::offset_of!(EnvironmentUniform, star_animation) == 320);
    assert!(std::mem::offset_of!(EnvironmentUniform, moon_orbit_world) == 336);
    assert!(std::mem::offset_of!(EnvironmentUniform, halo_count) == 352);
    assert!(std::mem::offset_of!(EnvironmentUniform, halo_sources) == 368);
};
