struct RainVertex {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) world_point: vec3<f32>,
    @location(1) @interpolate(flat) shelter_cell: u32,
    @location(2) @interpolate(flat) sparkle: f32,
};

@vertex fn vs_rain(@builtin(vertex_index) vertex: u32,
    @builtin(instance_index) instance: u32) -> RainVertex {
    let up = normalize(camera.eye_world.xyz);
    var east = vec3<f32>(0.0, 0.0, 1.0);
    if length(up.xz) > 0.00000001 { east = normalize(vec3<f32>(-up.z, 0.0, up.x)); }
    let north = cross(east, up);
    let cell = (instance * 137u) % 1024u;
    let grid = vec2<f32>(f32(cell % 32u), f32(cell / 32u)) - 16.0;
    let latitude = asin(clamp(up.y, -1.0, 1.0));
    let longitude = atan2(up.z, up.x);
    let anchor = vec2<f32>(longitude * weather.blend.y * cos(latitude),
        latitude * weather.blend.y);
    let seed = weather_hash(vec3<f32>(floor(anchor / 1.4) + grid, 1.0));
    let jitter = vec2<f32>(seed, fract(seed * 17.37)) - 0.5;
    let horizontal = (grid + jitter) * 1.4 - fract(anchor / 1.4) * 1.4;
    let snow = weather.wind.w <= 0.0;
    // A world cell keeps its fall speed when the moving emitter assigns it a new instance.
    let speed_class = f32(u32(seed * 1000.0) % 3u);
    let fall_rate = select(0.5 + speed_class * 0.25,
        0.1 + speed_class * 0.05, snow);
    let fall = fract(seed + weather.blend.z * fall_rate);
    let altitude = length(camera.eye_world.xyz) - weather.blend.y;
    let vertical_phase = fract(seed + weather.blend.z * fall_rate + altitude / 24.0);
    let center = camera.eye_world.xyz + east * horizontal.x + north * horizontal.y
        + up * (12.0 - vertical_phase * 24.0)
        + (east * weather.wind.x + north * weather.wind.y) * (fall * 0.18);
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, -0.5), vec2<f32>(0.5, -0.5), vec2<f32>(-0.5, 0.5),
        vec2<f32>(-0.5, 0.5), vec2<f32>(0.5, -0.5), vec2<f32>(0.5, 0.5));
    let corner = corners[vertex];
    let across = cross(normalize(center - camera.eye_world.xyz), up);
    let width = normalize(across + east * 0.001);
    let slant = normalize(-up + (east * weather.wind.x + north * weather.wind.y) *
        select(0.025, 0.08, snow));
    let world_point = center + width * corner.x * select(0.045, 0.10, snow)
        + slant * corner.y * select(0.75, 0.10, snow);
    var output: RainVertex;
    output.world_point = world_point;
    output.shelter_cell = (cell / 128u) * 8u + (cell % 32u) / 4u;
    // Stable per world-cell highlight; no frame-number glitter.
    output.sparkle = fract(seed * 97.37 + fract(seed * 13.71) * 0.29);
    output.clip_position = camera.view_projection * vec4<f32>(world_point - weather.origin.xyz, 1.0);
    return output;
}

@fragment fn fs_rain(input: RainVertex) -> @location(0) vec4<f32> {
    let altitude_m = (length(input.world_point) - weather.blend.y) / weather.wind.z;
    if altitude_m > weather.event.z { discard; }
    let word = u32(weather.shelter[input.shelter_cell / 16u]);
    if (word & (1u << (input.shelter_cell % 16u))) != 0u { discard; }
    // Emitter intensity is evaluated from the occupied cloud column at the
    // observer, using the same endpoints as the published cloud geometry.
    let intensity = clamp(weather.cover_rain.w / 32.0, 0.0, 1.0);
    if intensity < 0.002 { discard; }
    let view = normalize(input.world_point - camera.eye_world.xyz);
    let sun = normalize(environment.global_sun_direction.xyz);
    let shadow = cached_cloud_shadow(cloud_shadow_atlas, weather_sampler, input.world_point,
        sun, weather.event.y, weather.blend.y, weather.cloud_region);
    let toward_sun = max(dot(view, sun), 0.0);
    let ice = weather.wind.w <= 0.0;
    // Forward scattering from water droplets plus uncommon sharp glints.
    // Both are explicitly gated by illuminated Sun and cloud shadows.
    let broad_phase = 0.10 + 0.26 * pow(toward_sun, 12.0);
    let rare_glint = smoothstep(0.94, 0.995, input.sparkle)
        * pow(toward_sun, 64.0) * select(0.42, 0.08, ice);
    var moon_dir = vec3<f32>(0.0, 1.0, 0.0);
    let moon_length2 = dot(environment.moon_apparent_world.xyz,
        environment.moon_apparent_world.xyz);
    if moon_length2 > 1.0e-8 {
        moon_dir = normalize(environment.moon_apparent_world.xyz);
    }
    let moon_phase = 0.15 + 0.25 * pow(max(dot(view, moon_dir), 0.0), 8.0);
    let radiance = environment.ambient_rgb_exposure.xyz
        + environment.local_sun_rgb_visibility.xyz
            * (broad_phase + rare_glint) * shadow
        + environment.local_moon_rgb_visibility.xyz * moon_phase;
    let luminance = dot(radiance, vec3<f32>(0.2126, 0.7152, 0.0722));
    let flash = weather.lightning.w * 0.65;
    let brightness = clamp(sqrt(max(luminance, 0.0)) * 0.7 + flash, 0.0, 0.95);
    let color = select(vec3<f32>(0.74, 0.80, 0.86),
        vec3<f32>(0.95, 0.96, 1.0), weather.wind.w <= 0.0) * brightness;
    let distance = length(input.world_point - camera.eye_world.xyz);
    let height = abs(dot(input.world_point - camera.eye_world.xyz, normalize(camera.eye_world.xyz)));
    let fade = (1.0 - smoothstep(18.0, 27.0, distance)) * (1.0 - smoothstep(9.0, 12.0, height));
    let alpha = (0.22 + sqrt(intensity) * 0.22) * fade * smoothstep(0.0, 0.05, brightness);
    return vec4<f32>(color * alpha, alpha);
}
