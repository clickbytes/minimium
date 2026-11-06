struct AtmosphereUniform {
    radii: vec4<f32>,
    rayleigh: vec4<f32>,
    mie: vec4<f32>,
    ozone: vec4<f32>,
    config: vec4<f32>,
    up_height: vec4<f32>,
    eye_relative: vec4<f32>,
    weather: vec4<f32>,
    fog_center: vec4<f32>,
    fog_east: vec4<f32>,
    fog_north: vec4<f32>,
    fog_time: vec4<f32>,
    inverse_view_projection: mat4x4<f32>,
};
@group(0) @binding(2) var<uniform> atmosphere: AtmosphereUniform;
@group(0) @binding(3) var atmosphere_table: texture_2d<f32>;
@group(0) @binding(4) var atmosphere_sampler: sampler;
@group(0) @binding(5) var sky_scattering: texture_2d<f32>;
@group(0) @binding(6) var sky_transmission: texture_2d<f32>;
@group(0) @binding(7) var aerial_scattering: texture_3d<f32>;
@group(0) @binding(8) var aerial_transmission: texture_3d<f32>;
@group(0) @binding(9) var multiple_scattering: texture_2d<f32>;
@group(0) @binding(10) var atmosphere_fog: texture_2d_array<f32>;
fn physical_atmosphere() -> bool { return atmosphere.config.w > 0.5 && environment.flags.z != 0u; }
// Keep night radiance above half-float underflow without changing light ratios.
fn scene_exposure() -> f32 { return max(environment.ambient_rgb_exposure.w,0.00001); }
fn surface_moon_direction() -> vec3<f32> {
    return normalize(environment.moon_orbit_world.xyz-camera.eye_world.xyz);
}
fn celestial_contrast() -> f32 {
    var high=0.0;
    if atmosphere.config.x>0.0 {high=smoothstep(12000.0,80000.0,atmosphere.up_height.w);}
    return mix(environment.sky_horizon_rgb_star_visibility.w,1.0,high);
}
fn visual_moon_direction() -> vec3<f32> {
    let apparent=normalize(environment.moon_apparent_world.xyz);
    if !physical_atmosphere() {return apparent;}
    let geometric=normalize(environment.moon_orbit_world.xyz-camera.eye_world.xyz);
    let high=smoothstep(60000.0,100000.0,atmosphere.up_height.w);
    return normalize(mix(apparent,geometric,high));
}
fn atmosphere_ray(uv: vec2<f32>) -> vec3<f32> {
    let p = camera.inverse_sky_view_projection*vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),1.0,1.0);
    return normalize(p.xyz/max(abs(p.w),1e-8));
}
struct AirPath { scattering: vec3<f32>, transmission: vec3<f32>, fog: f32 };
fn sample_aerial(local_position: vec3<f32>) -> AirPath {
    let clip = camera.view_projection*vec4<f32>(local_position,1.0);
    let uv = clip.xy/max(clip.w,1e-8)*vec2<f32>(0.5,-0.5)+0.5;
    let distance = length(local_position-atmosphere.eye_relative.xyz)*atmosphere.config.y;
    let z = log(1.0+distance)/log(1.0+atmosphere.eye_relative.w);
    let edge=select(16.0,32.0,atmosphere.config.z>0.5);
    let coordinate = (clamp(vec3<f32>(uv,z),vec3<f32>(0.0),vec3<f32>(1.0))*vec3<f32>(edge-1.0,edge-1.0,31.0)+0.5)/32.0;
    let scattering=textureSampleLevel(aerial_scattering,atmosphere_sampler,coordinate,0.0);
    return AirPath(scattering.rgb/scene_exposure(),
        textureSampleLevel(aerial_transmission,atmosphere_sampler,coordinate,0.0).rgb,scattering.a);
}

fn sample_sky(uv:vec2<f32>)->AirPath {
    let size=select(vec2<f32>(128.0,64.0),vec2<f32>(192.0,108.0),atmosphere.config.z>0.5);
    let coordinate=(clamp(uv,vec2<f32>(0.0),vec2<f32>(1.0))*(size-1.0)+0.5)/vec2<f32>(192.0,108.0);
    let scattering=textureSampleLevel(sky_scattering,atmosphere_sampler,coordinate,0.0);
    return AirPath(scattering.rgb/scene_exposure(),
        textureSampleLevel(sky_transmission,atmosphere_sampler,coordinate,0.0).rgb,scattering.a);
}
