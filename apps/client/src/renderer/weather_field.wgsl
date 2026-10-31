const WEATHER_SNAPSHOT_LAYERS: i32 = 16;

fn weather_uv(point: vec3<f32>) -> vec2<f32> {
    let direction = normalize(point);
    return vec2<f32>(fract(atan2(direction.z, direction.x) / 6.2831853 + 0.5),
        clamp(0.5 - asin(clamp(direction.y, -1.0, 1.0)) / 3.14159265, 0.0, 1.0));
}

fn weather_region(point: vec3<f32>, region: vec4<f32>) -> vec3<f32> {
    let up = normalize(point);
    let facing = dot(up, region.xyz);
    if region.w <= 0.0 || facing <= 0.0 { return vec3<f32>(0.5, 0.5, 0.0); }
    var east = vec3<f32>(0.0, 0.0, 1.0);
    if length(region.xz) > 0.00000001 { east = normalize(vec3<f32>(-region.z, 0.0, region.x)); }
    let north = cross(east, region.xyz);
    let uv = vec2<f32>(0.5 + dot(up, east) / facing / region.w * 0.5,
        0.5 - dot(up, north) / facing / region.w * 0.5);
    let weight = 1.0 - smoothstep(0.25, 0.5, max(abs(uv.x - 0.5), abs(uv.y - 0.5)));
    return vec3<f32>(uv, weight);
}

fn weather_channel(atlas: texture_2d_array<f32>, sampler_state: sampler,
    point: vec3<f32>, blend: f32, region: vec4<f32>, channel: i32) -> vec4<f32> {
    let uv = weather_uv(point);
    let a = textureSampleLevel(atlas, sampler_state, uv, channel, 0.0);
    let b = textureSampleLevel(atlas, sampler_state, uv, channel + 1, 0.0);
    let global = mix(a, b, blend);
    let regional = weather_region(point, region);
    if regional.z <= 0.0 { return global; }
    // The common sampler repeats longitude. Keep regional samples inside edge texels.
    let local_uv = clamp(regional.xy, vec2<f32>(0.5 / 512.0, 0.5 / 256.0),
        vec2<f32>(1.0 - 0.5 / 512.0, 1.0 - 0.5 / 256.0));
    let c = textureSampleLevel(atlas, sampler_state, local_uv, channel + WEATHER_SNAPSHOT_LAYERS, 0.0);
    let d = textureSampleLevel(atlas, sampler_state, local_uv, channel + WEATHER_SNAPSHOT_LAYERS + 1, 0.0);
    return mix(global, mix(c, d, blend), regional.z);
}

fn weather_conditions(atlas: texture_2d_array<f32>, sampler_state: sampler,
    point: vec3<f32>, blend: f32, region: vec4<f32>) -> vec4<f32> {
    return weather_channel(atlas, sampler_state, point, blend, region, 0);
}

fn weather_auxiliary(atlas: texture_2d_array<f32>, sampler_state: sampler,
    point: vec3<f32>, blend: f32, region: vec4<f32>) -> vec4<f32> {
    return weather_channel(atlas, sampler_state, point, blend, region, 2);
}

fn weather_dynamics(atlas: texture_2d_array<f32>, sampler_state: sampler,
    point: vec3<f32>, blend: f32, region: vec4<f32>) -> vec4<f32> {
    return weather_channel(atlas, sampler_state, point, blend, region, 4);
}

fn weather_hash(cell: vec3<f32>) -> f32 {
    // Integer mixing avoids large-argument sine precision loss on planetary coordinates.
    let key = bitcast<vec3<u32>>(vec3<i32>(floor(cell)));
    var value = key.x * 0x8da6b343u ^ key.y * 0xd8163841u ^ key.z * 0xcb1ab31fu;
    value = (value ^ (value >> 16u)) * 0x7feb352du;
    value = (value ^ (value >> 15u)) * 0x846ca68bu;
    value = value ^ (value >> 16u);
    return f32(value >> 8u) * (1.0 / 16777216.0);
}

fn weather_noise(point: vec3<f32>) -> f32 {
    let cell = floor(point);
    let t = fract(point);
    let w = t * t * (3.0 - 2.0 * t);
    let a = mix(weather_hash(cell), weather_hash(cell + vec3<f32>(1.0, 0.0, 0.0)), w.x);
    let b = mix(weather_hash(cell + vec3<f32>(0.0, 1.0, 0.0)),
        weather_hash(cell + vec3<f32>(1.0, 1.0, 0.0)), w.x);
    let c = mix(weather_hash(cell + vec3<f32>(0.0, 0.0, 1.0)),
        weather_hash(cell + vec3<f32>(1.0, 0.0, 1.0)), w.x);
    let d = mix(weather_hash(cell + vec3<f32>(0.0, 1.0, 1.0)),
        weather_hash(cell + vec3<f32>(1.0, 1.0, 1.0)), w.x);
    return mix(mix(a, b, w.y), mix(c, d, w.y), w.z);
}
