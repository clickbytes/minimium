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
    relief_tile: vec4<f32>,
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
@group(0) @binding(12) var sky_irradiance: texture_2d<f32>;
fn physical_atmosphere() -> bool { return atmosphere.config.w > 0.5 && environment.flags.z != 0u; }
fn has_spherical_atmosphere() -> bool { return atmosphere.config.x>0.0 && environment.flags.z!=0u; }
// Keep night radiance above half-float underflow without changing light ratios.
fn scene_exposure() -> f32 { return max(environment.ambient_rgb_exposure.w,0.00001); }
// Incoming direction at this receiver and sine of the physical disk radius.
// Match Astronomy Engine's standard Saemundsson correction at sea level.
// Scale by air density so a vacuum observer sees the geometric center.
fn refracted_direction(direction:vec3<f32>, up:vec3<f32>, height:f32)->vec3<f32> {
    let cosine=clamp(dot(up,direction),-1.0,1.0);
    let tangent=up-cosine*direction;
    let tangent_sq=dot(tangent,tangent);
    if tangent_sq<1e-8 || height>=atmosphere.radii.y || atmosphere.radii.z<=0.0 {
        return direction;
    }
    let altitude=degrees(asin(cosine));
    let hd=max(altitude,-1.0);
    let lift=radians(1.02/60.0)/tan(radians(hd+10.3/(hd+5.11)))
        *clamp((altitude+90.0)/89.0,0.0,1.0)*exp(-max(height,0.0)/atmosphere.radii.z);
    return direction*cos(lift)+tangent*(sin(lift)*inverseSqrt(tangent_sq));
}
fn lunar_ray(to_moon: vec3<f32>, up: vec3<f32>, height: f32) -> vec4<f32> {
    let distance=length(to_moon);
    if environment.moon_orbit_world.w<=0.0 || distance<1.0 {return vec4<f32>(0.0);}
    return vec4<f32>(refracted_direction(to_moon/distance,up,height),
        clamp(environment.moon_orbit_world.w/distance,1e-7,1.0));
}
fn solar_direction(up:vec3<f32>,height:f32)->vec3<f32> {
    if environment.sun_apparent_world.w<=0.0 {return vec3<f32>(0.0);}
    return refracted_direction(environment.global_sun_direction.xyz,up,height);
}
fn visual_sun_direction()->vec3<f32> {
    if !has_spherical_atmosphere() { return environment.sun_apparent_world.xyz; }
    return solar_direction(atmosphere.up_height.xyz,atmosphere.up_height.w);
}
fn visual_moon_ray() -> vec4<f32> {
    if !has_spherical_atmosphere() {
        return vec4<f32>(environment.moon_apparent_world.xyz,sin(environment.moon_apparent_world.w));
    }
    return lunar_ray(environment.moon_orbit_world.xyz-camera.eye_world.xyz,
        atmosphere.up_height.xyz,atmosphere.up_height.w);
}
fn atmosphere_ray(uv: vec2<f32>) -> vec3<f32> {
    let p = camera.inverse_sky_view_projection*vec4<f32>(uv*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),1.0,1.0);
    return normalize(p.xyz/max(abs(p.w),1e-8));
}
struct AirPath { scattering: vec3<f32>, transmission: vec3<f32>, fog: f32, shadowed_fog: f32 };
fn clear_air_path() -> AirPath {
    return AirPath(vec3<f32>(0.0),vec3<f32>(1.0),0.0,0.0);
}
fn join_air_paths(near: AirPath, far: AirPath) -> AirPath {
    return AirPath(near.scattering+near.transmission*far.scattering,
        near.transmission*far.transmission,near.fog+far.fog,near.shadowed_fog+far.shadowed_fog);
}
fn aerial_size() -> vec3<u32> {
    return select(vec3<u32>(32,18,32),vec3<u32>(96,54,48),atmosphere.config.z>0.5);
}
fn aerial_distance(z: f32) -> f32 {
    return exp(z*log(1.0+atmosphere.eye_relative.w))-1.0;
}
// The sky shares the same foreground as opaque and transparent geometry.
// Stop on a stored slice so a partially interpolated interval is never counted twice.
fn sky_prefix_z() -> f32 {
    let slices=f32(aerial_size().z-1u);
    let distance=min(2048.0*atmosphere.config.y,atmosphere.eye_relative.w);
    return floor(slices*log(1.0+distance)/log(1.0+atmosphere.eye_relative.w))/slices;
}
fn aerial_path(uv: vec2<f32>, z: f32) -> AirPath {
    let active_size=vec3<f32>(aerial_size());
    let coordinate=(clamp(vec3<f32>(uv,z),vec3<f32>(0.0),vec3<f32>(1.0))*(active_size-1.0)+0.5)
        /vec3<f32>(textureDimensions(aerial_scattering));
    let scattering=textureSampleLevel(aerial_scattering,atmosphere_sampler,coordinate,0.0);
    let transmission=textureSampleLevel(aerial_transmission,atmosphere_sampler,coordinate,0.0);
    return AirPath(scattering.rgb/scene_exposure(),transmission.rgb,scattering.a,transmission.a);
}
fn sample_aerial(local_position: vec3<f32>) -> AirPath {
    let clip = camera.view_projection*vec4<f32>(local_position,1.0);
    let uv = clip.xy/max(clip.w,1e-8)*vec2<f32>(0.5,-0.5)+0.5;
    let distance = length(local_position-atmosphere.eye_relative.xyz)*atmosphere.config.y;
    let z = log(1.0+distance)/log(1.0+atmosphere.eye_relative.w);
    return aerial_path(uv,z);
}

fn sample_sky(uv:vec2<f32>)->AirPath {
    let size=select(vec2<f32>(128.0,64.0),vec2<f32>(192.0,108.0),atmosphere.config.z>0.5);
    let coordinate=(clamp(uv,vec2<f32>(0.0),vec2<f32>(1.0))*(size-1.0)+0.5)/vec2<f32>(192.0,108.0);
    let scattering=textureSampleLevel(sky_scattering,atmosphere_sampler,coordinate,0.0);
    let transmission=textureSampleLevel(sky_transmission,atmosphere_sampler,coordinate,0.0);
    let far=AirPath(scattering.rgb/scene_exposure(),transmission.rgb,scattering.a,transmission.a);
    return join_air_paths(aerial_path(uv,sky_prefix_z()),far);
}
