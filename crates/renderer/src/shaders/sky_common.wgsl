struct SkySample {
    scattering_rgb: vec3<f32>,
    transmittance: f32,
};

const SOLAR_DISK_RADIANCE = vec3<f32>(8.0, 6.5, 4.2);
const SOLAR_HALO_FACTOR = 0.12;
const HORIZON_OCCLUSION_RGB = vec3<f32>(0.008, 0.010, 0.014);
const CLEAR_AIR_EXTINCTION = 0.08;

// Finite emitter-to-receiver clear-air path, in the same optical metres and
// extinction coefficients as the atmosphere. No LUT is required in fallback.
fn bolid_air_transmission(receiver: vec3<f32>, emitter: vec3<f32>) -> vec3<f32> {
    if atmosphere.config.x <= 0.0 || atmosphere.config.y <= 0.0 {
        return vec3<f32>(1.0);
    }
    let scale = atmosphere.config.y;
    let radius = atmosphere.radii.x;
    let delta = (emitter-receiver)*scale;
    let distance = length(delta);
    if distance < 0.001 { return vec3<f32>(1.0); }
    let ray = delta/distance;
    let height = (length(receiver)-atmosphere.config.x)*scale;
    let start = normalize(receiver)*(radius+height);
    let b = dot(start, ray);
    let c = height*(2.0*radius+height);
    // Spend the bounded samples inside air, not on potentially long orbital
    // vacuum legs. This also handles two exterior endpoints across the limb.
    let top = atmosphere.radii.y;
    let c_top = (height-top)*(2.0*radius+height+top);
    let discriminant = b*b-c_top;
    if discriminant <= 0.0 { return vec3<f32>(1.0); }
    let root = sqrt(discriminant);
    let begin = max(-b-root,0.0);
    let end = min(-b+root,distance);
    if end <= begin { return vec3<f32>(1.0); }
    let closest = clamp(-b, begin, end);
    var left_steps = 4u;
    if closest <= begin { left_steps = 0u; }
    if closest >= end { left_steps = 8u; }
    var optical_depth = vec3<f32>(0.0);
    for (var i=0u; i<8u; i++) {
        var a = 0.0;
        var z = 0.0;
        if i < left_steps {
            let u = 1.0-f32(i)/f32(left_steps);
            let v = 1.0-f32(i+1u)/f32(left_steps);
            a = closest-(closest-begin)*u*u*u;
            z = closest-(closest-begin)*v*v*v;
        } else {
            let count = f32(max(8u-left_steps,1u));
            let u = f32(i-left_steps)/count;
            let v = f32(i+1u-left_steps)/count;
            a = closest+(end-closest)*u*u*u;
            z = closest+(end-closest)*v*v*v;
        }
        let d = (a+z)*0.5;
        let difference = c+2.0*b*d+d*d;
        let h = max(difference/(sqrt(max(radius*radius+difference,1.0))+radius),0.0);
        if h < atmosphere.radii.y {
            let rayleigh = atmosphere.rayleigh.xyz*exp(-h/max(atmosphere.radii.z,1.0));
            let aerosol = atmosphere.mie.x*exp(-h/max(atmosphere.radii.w,1.0));
            let ozone = max(1.0-abs(h-atmosphere.ozone.w)/max(atmosphere.mie.z,1.0),0.0);
            optical_depth += (rayleigh+vec3<f32>(aerosol)+atmosphere.ozone.xyz*ozone)*(z-a);
        }
    }
    return exp(-optical_depth);
}

fn bolid_incident(local_position: vec3<f32>) -> vec3<f32> {
    if environment.bolid_position_intensity.w <= 0.0 { return vec3<f32>(0.0); }
    let delta = environment.bolid_position_intensity.xyz
        - (local_position + environment.render_origin.xyz);
    let distance_squared = max(dot(delta, delta), 10000.0);
    let point = local_position + environment.render_origin.xyz;
    let radius = atmosphere.config.x;
    let ray = delta / sqrt(distance_squared);
    let b = dot(point, ray);
    let c = (length(point)-radius)*(length(point)+radius);
    let discriminant = b*b-c;
    if radius > 0.0 && discriminant >= 0.0 {
        let hit = -b-sqrt(discriminant);
        if hit > 1.0 && hit < sqrt(distance_squared) { return vec3<f32>(0.0); }
        if b < 0.0 && c <= 0.0 { return vec3<f32>(0.0); }
    }
    return environment.bolid_rgb.xyz * environment.bolid_position_intensity.w
        * environment.bolid_rgb.w * (10000000000.0 / distance_squared)
        * bolid_air_transmission(point, environment.bolid_position_intensity.xyz);
}

fn bolid_irradiance(local_position: vec3<f32>, normal: vec3<f32>) -> vec3<f32> {
    let delta = environment.bolid_position_intensity.xyz
        - (local_position + environment.render_origin.xyz);
    let cosine=dot(normal,normalize(delta));
    if cosine<=0.0 {return vec3<f32>(0.0);}
    return bolid_incident(local_position) * cosine;
}

fn tone_map_linear(rgb: vec3<f32>) -> vec3<f32> {
    if environment.flags.w != 0u {
        return min(rgb*scene_exposure(),vec3<f32>(65000.0));
    }
    return tone_map_color(rgb * environment.ambient_rgb_exposure.w);
}

// Existing celestial sprites/marker colors are calibrated for display, unlike
// irradiance on surfaces. Do not amplify those authored colors by night gain.
fn tone_map_display(rgb: vec3<f32>) -> vec3<f32> {
    if environment.flags.w != 0u {return rgb;}
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
    var density=1.0;
    if atmosphere.config.x>0.0 {density=exp(-max(atmosphere.up_height.w,0.0)/8000.0);}
    return SkySample(scattering*density, pow(clear_air_transmittance(altitude),density));
}
