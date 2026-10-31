// Bounded single scattering with a precomputed spherical optical-depth table.
// No weather noise, cloud-density march or view-dependent history here.
const AIR_RADIUS: f32 = 6371000.0;
const AIR_TOP: f32 = 100000.0;
const AIR_RAYLEIGH: vec3<f32> = vec3<f32>(5.802e-6, 13.558e-6, 33.100e-6);
const AIR_OZONE: vec3<f32> = vec3<f32>(0.650e-6, 1.881e-6, 0.085e-6);

fn air_light(table: texture_2d<f32>, table_sampler: sampler,
    point: vec3<f32>, light: vec3<f32>, radius: f32) -> vec3<f32> {
    let height = clamp((length(point) / radius - 1.0) * AIR_RADIUS, 0.0, AIR_TOP);
    let r = AIR_RADIUS + height;
    let horizon = -sqrt(max(1.0 - pow(AIR_RADIUS / r, 2.0), 0.0));
    let cosine = dot(normalize(point), light);
    let disk = smoothstep(horizon - 0.00465, horizon + 0.00465, cosine);
    let u = sqrt(clamp((cosine - horizon) / (1.0 - horizon), 0.0, 1.0));
    let v = sqrt(height / AIR_TOP);
    let uv = (vec2<f32>(u, v) * vec2<f32>(127.0, 63.0) + 0.5) / vec2<f32>(128.0, 64.0);
    return exp(-textureSampleLevel(table, table_sampler, uv, 0.0).rgb) * disk;
}

fn global_moon_direction(point: vec3<f32>) -> vec3<f32> {
    return normalize(environment.moon_orbit_world.xyz - point);
}

fn global_moon_irradiance() -> vec3<f32> {
    // Disk radiance is NOT ground illuminance. Relative full-Moon/Sun irradiance
    // agrees with the environment's 2e-6 moonlight profile.
    return vec3<f32>(0.25, 0.32, 0.5) * (2.0e-6 * environment.moon_disk_radiance.w);
}

struct AirPath {
    scattering: vec3<f32>,
    transmission: vec3<f32>,
};

fn air_path(table: texture_2d<f32>, table_sampler: sampler, eye_world: vec3<f32>,
    ray: vec3<f32>, distance_world: f32, radius: f32, steps: u32, readable: f32) -> AirPath {
    let scale = AIR_RADIUS / max(radius, 1.0);
    let eye = eye_world * scale;
    let b = dot(eye, ray);
    let discriminant = b * b - dot(eye, eye) + pow(AIR_RADIUS + AIR_TOP, 2.0);
    var result = AirPath(vec3<f32>(0.0), vec3<f32>(1.0));
    if discriminant <= 0.0 { return result; }
    let root = sqrt(discriminant);
    let begin = max(-b - root, 0.0);
    let end = min(-b + root, distance_world * scale);
    if end <= begin { return result; }
    let sun = normalize(environment.global_sun_direction.xyz);
    let cosine = dot(ray, sun);
    let rayleigh_phase = 3.0 / (16.0 * 3.14159265) * (1.0 + cosine * cosine);
    let g = 0.76;
    let mie_phase = (1.0 - g * g) / (4.0 * 3.14159265 * pow(1.0 + g * g - 2.0 * g * cosine, 1.5));
    let ds = (end - begin) / f32(steps);
    for (var i = 0u; i < steps; i++) {
        let point = eye + ray * (begin + (f32(i) + 0.5) * ds);
        let height = max(length(point) - AIR_RADIUS, 0.0);
        let rayleigh = AIR_RAYLEIGH * exp(-height / 8000.0);
        let mie = exp(-height / 1200.0);
        let ozone = max(1.0 - abs(height - 25000.0) / 15000.0, 0.0);
        let extinction = rayleigh + vec3<f32>(4.44e-6 * mie) + AIR_OZONE * ozone;
        let segment = exp(-extinction * ds);
        let light = air_light(table, table_sampler, point, sun, AIR_RADIUS);
        let moon_direction = global_moon_direction(point / scale);
        let moon = global_moon_irradiance() * max(dot(normalize(point), moon_direction), 0.0);
        // Approximate diffuse multiple scattering; direct scattering retains the
        // spherical terminator. No uniform blue ring on the physical night side.
        let solar_fill = smoothstep(-0.12, 0.2, dot(normalize(point), sun)) * 0.012;
        let source = (rayleigh * rayleigh_phase + vec3<f32>(3.996e-6 * mie * mie_phase))
            * light * environment.global_sun_rgb.xyz
            + rayleigh * (solar_fill + moon + vec3<f32>(0.006, 0.012, 0.025) * readable);
        let integrated = source / max(extinction, vec3<f32>(1e-12)) * (1.0 - segment);
        let glow_layer = exp(-pow((height - 87000.0) / 7000.0, 2.0));
        let glow = vec3<f32>(0.25, 1.0, 0.35) * glow_layer * ds * 1e-11;
        result.scattering += result.transmission * (integrated + glow);
        result.transmission *= segment;
    }
    return result;
}

fn air_surface(table: texture_2d<f32>, table_sampler: sampler, point: vec3<f32>,
    color: vec3<f32>, radius: f32, steps: u32, readable: f32) -> vec3<f32> {
    let offset = point - camera.eye_world.xyz;
    let air = air_path(table, table_sampler, camera.eye_world.xyz, normalize(offset),
        length(offset), radius, steps, readable);
    return color * air.transmission + air.scattering;
}
