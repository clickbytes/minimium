struct CameraUniform {
    view_projection: mat4x4<f32>,
    sky_view_projection: mat4x4<f32>,
    inverse_sky_view_projection: mat4x4<f32>,
    viewport: vec4<f32>,
};
@group(0) @binding(0) var<uniform> camera: CameraUniform;

struct EnvironmentUniform {
    sun_geometric_world: vec4<f32>,
    sun_apparent_world: vec4<f32>,
    moon_geometric_world: vec4<f32>,
    moon_apparent_world: vec4<f32>,
    moon_light_world_phase: vec4<f32>,
    moon_pole_world: vec4<f32>,
    local_sun_rgb_visibility: vec4<f32>,
    local_moon_rgb_visibility: vec4<f32>,
    global_sun_direction: vec4<f32>,
    global_sun_rgb: vec4<f32>,
    ambient_rgb_exposure: vec4<f32>,
    sky_zenith_rgb_sun_altitude: vec4<f32>,
    sky_horizon_rgb_star_visibility: vec4<f32>,
    observer_up_horizon_cos: vec4<f32>,
    flags: vec4<u32>,
    eqj_to_world: mat4x4<f32>,
    moon_disk_radiance: vec4<f32>,
    star_animation: vec4<f32>,
};
@group(0) @binding(1) var<uniform> environment: EnvironmentUniform;
