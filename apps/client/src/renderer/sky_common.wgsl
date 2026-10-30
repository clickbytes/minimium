struct SkySample {
    scattering_rgb: vec3<f32>,
    transmittance: f32,
};

const SOLAR_DISK_RADIANCE = vec3<f32>(8.0, 6.5, 4.2);
const SOLAR_HALO_FACTOR = 0.12;
const HORIZON_OCCLUSION_RGB = vec3<f32>(0.008, 0.010, 0.014);
const CLEAR_AIR_EXTINCTION = 0.08;

fn tone_map_linear(rgb: vec3<f32>) -> vec3<f32> {
    return tone_map_color(rgb);
}

fn horizon_visibility(ray_world: vec3<f32>) -> f32 {
    let up = normalize(environment.observer_up_horizon_cos.xyz);
    let cosine = dot(up, normalize(ray_world));
    let horizon = environment.observer_up_horizon_cos.w;
    return smoothstep(horizon - 0.0015, horizon + 0.0015, cosine);
}

fn clear_air_transmittance(altitude: f32) -> f32 {
    let zenith_degrees = clamp(90.0 - degrees(max(altitude, radians(-1.0))), 0.0, 90.0);
    let correction = 0.50572 * pow(max(96.07995 - zenith_degrees, 1.0), -1.6364);
    let air_mass = clamp(1.0 / max(sin(altitude) + correction, 0.025), 1.0, 40.0);
    return exp(-CLEAR_AIR_EXTINCTION * air_mass);
}

fn sample_clear_sky(ray_world: vec3<f32>) -> SkySample {
    let ray = normalize(ray_world);
    let up = normalize(environment.observer_up_horizon_cos.xyz);
    let altitude = asin(clamp(dot(up, ray), -1.0, 1.0));
    let horizon_weight = pow(clamp(1.0 - max(sin(altitude), 0.0), 0.0, 1.0), 1.35);
    let zenith = environment.sky_zenith_rgb_sun_altitude.xyz;
    let horizon = environment.sky_horizon_rgb_star_visibility.xyz;
    let sun_altitude = environment.sky_zenith_rgb_sun_altitude.w;
    let day_factor = smoothstep(radians(-18.0), radians(4.0), sun_altitude);
    let sun_direction = normalize(environment.sun_apparent_world.xyz);
    let sun_angle = acos(clamp(dot(ray, sun_direction), -1.0, 1.0));
    let horizon_glow = exp(-sun_angle * 7.0) * day_factor;
    var scattering = mix(zenith, horizon, horizon_weight)
        + vec3<f32>(0.34, 0.16, 0.055) * horizon_glow * 0.28;
    let moon_direction = environment.moon_apparent_world.xyz;
    if environment.local_moon_rgb_visibility.w > 0.0 && length(moon_direction) > 1.0e-8 {
        let moon_angle = acos(clamp(dot(ray, normalize(moon_direction)), -1.0, 1.0));
        let halo_width = max(radians(7.0), environment.moon_apparent_world.w * 16.0);
        let moon_halo = exp(-moon_angle / halo_width);
        let moon_scatter = environment.local_moon_rgb_visibility.xyz
            * (vec3<f32>(18.0, 24.0, 42.0) + vec3<f32>(44.0, 58.0, 86.0) * moon_halo);
        scattering += moon_scatter * clear_air_transmittance(altitude);
    }
    return SkySample(scattering, clear_air_transmittance(altitude));
}
