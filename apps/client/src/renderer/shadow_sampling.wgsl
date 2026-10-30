struct ShadowUniform {
    light_view_projection: mat4x4<f32>,
    receiver_center_relative_radius: vec4<f32>,
    texel_depth_range: vec4<f32>,
    bias_fade: vec4<f32>,
    flags: vec4<u32>,
};

@group(2) @binding(0) var shadow_depth: texture_depth_2d;
@group(2) @binding(1) var shadow_sampler: sampler_comparison;
@group(2) @binding(2) var<uniform> shadow: ShadowUniform;

fn shadow_visibility(world_position: vec3<f32>, normal: vec3<f32>, light_direction: vec3<f32>) -> f32 {
    if shadow.flags.x == 0u || shadow.flags.y == 0u {
        return 1.0;
    }

    let receiver_offset = world_position - shadow.receiver_center_relative_radius.xyz;
    let receiver_distance = length(receiver_offset);
    let receiver_radius = max(shadow.receiver_center_relative_radius.w, 0.0);
    if receiver_distance >= receiver_radius {
        return 1.0;
    }

    let clip = shadow.light_view_projection * vec4<f32>(world_position, 1.0);
    if clip.w <= 0.000001 {
        return 1.0;
    }
    let ndc = clip.xyz / clip.w;
    if ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 || ndc.z < 0.0 || ndc.z > 1.0 {
        return 1.0;
    }

    let uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let slope = 1.0 - clamp(dot(normalize(normal), normalize(light_direction)), 0.0, 1.0);
    let bias_m = min(shadow.bias_fade.z, shadow.bias_fade.x + shadow.bias_fade.y * slope);
    let depth_range_m = max(shadow.texel_depth_range.w - shadow.texel_depth_range.z, 0.000001);
    let reference_depth = ndc.z - bias_m / depth_range_m;
    let texel = shadow.texel_depth_range.xy;
    var pcf_sum = 0.0;
    var sample_count = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let sample_uv = uv + vec2<f32>(f32(x), f32(y)) * texel;
            if sample_uv.x < 0.0 || sample_uv.x > 1.0 || sample_uv.y < 0.0 || sample_uv.y > 1.0 {
                pcf_sum += 1.0;
            } else {
                pcf_sum += textureSampleCompare(shadow_depth, shadow_sampler, sample_uv, reference_depth);
            }
            sample_count += 1.0;
        }
    }

    let pcf_visibility = pcf_sum / max(sample_count, 1.0);
    let fade_start = min(shadow.bias_fade.w, receiver_radius);
    let fade = 1.0 - smoothstep(fade_start, receiver_radius, receiver_distance);
    return mix(1.0, pcf_visibility, fade);
}
