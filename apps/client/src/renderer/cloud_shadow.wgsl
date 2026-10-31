// Four opacity/rain images, followed by four actual top-height images.
// Global/local regions, each at the two physical field endpoints.
// No density noise or secondary volume ray marches in surface materials.
fn cached_cloud_weather(atlas: texture_2d_array<f32>, sampler_state: sampler,
    point: vec3<f32>, morph: f32, region: vec4<f32>) -> vec4<f32> {
    return cached_cloud_channel(atlas, sampler_state, point, morph, region, 0);
}

fn cached_cloud_channel(atlas: texture_2d_array<f32>, sampler_state: sampler,
    point: vec3<f32>, morph: f32, region: vec4<f32>, channel: i32) -> vec4<f32> {
    let uv = weather_uv(point);
    let a = textureSampleLevel(atlas, sampler_state, uv, channel, 0.0);
    let b = textureSampleLevel(atlas, sampler_state, uv, channel + 1, 0.0);
    let global = mix(a, b, morph);
    let local = weather_region(point, region);
    if local.z <= 0.0 { return global; }
    let half_texel = vec2<f32>(0.5) / vec2<f32>(textureDimensions(atlas));
    let local_uv = clamp(local.xy, half_texel, vec2<f32>(1.0) - half_texel);
    let c = textureSampleLevel(atlas, sampler_state, local_uv, channel + 2, 0.0);
    let d = textureSampleLevel(atlas, sampler_state, local_uv, channel + 3, 0.0);
    return mix(global, mix(c, d, morph), local.z);
}

fn cached_cloud_opacity(atlas: texture_2d_array<f32>, sampler_state: sampler,
    point: vec3<f32>, morph: f32, region: vec4<f32>) -> vec3<f32> {
    return cached_cloud_weather(atlas, sampler_state, point, morph, region).rgb;
}

fn cached_cloud_shadow(atlas: texture_2d_array<f32>, sampler_state: sampler,
    point: vec3<f32>, light: vec3<f32>, morph: f32, radius: f32, region: vec4<f32>) -> f32 {
    if radius <= 0.0 { return 1.0; }
    let scale = radius / 6371000.0;
    let up = normalize(point);
    let cosine = dot(up, light);
    if cosine <= -0.03 { return 1.0; }
    let heights = cached_cloud_channel(atlas, sampler_state, point, morph, region, 4).rgb * 20000.0;
    var transmittance = 1.0;
    for (var layer = 0u; layer < 3u; layer += 1u) {
        let r = radius + heights[layer] * scale;
        let b = dot(point, light);
        let distance = -b + sqrt(max(b * b - dot(point, point) + r * r, 0.0));
        if distance <= 0.0 { continue; }
        let sample = point + light * distance;
        let opacity = cached_cloud_opacity(atlas, sampler_state, sample, morph, region)[layer];
        transmittance *= pow(max(1.0 - opacity, 0.005), 1.0 / max(cosine, 0.2));
    }
    return transmittance;
}
