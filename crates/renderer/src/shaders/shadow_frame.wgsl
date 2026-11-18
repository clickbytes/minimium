struct ShadowUniform {
    light_view_projection: mat4x4<f32>,
    receiver_center_relative_radius: vec4<f32>,
    texel_depth_range: vec4<f32>,
    bias_fade: vec4<f32>,
    flags: vec4<u32>,
};

@group(2) @binding(0) var shadow_depth: texture_depth_2d_array;
@group(2) @binding(1) var shadow_sampler: sampler_comparison;
@group(2) @binding(2) var<uniform> shadow: ShadowUniform;
@group(2) @binding(5) var<uniform> secondary_moon: ShadowUniform;
@group(2) @binding(6) var<uniform> secondary_landscape: ShadowUniform;
@group(2) @binding(4) var<uniform> landscape_shadow: ShadowUniform;

fn shadow_coverage(cascade: ShadowUniform, world_position: vec3<f32>) -> f32 {
    if cascade.flags.x == 0u { return 0.0; }
    let distance = length(world_position - cascade.receiver_center_relative_radius.xyz);
    return 1.0 - smoothstep(cascade.bias_fade.w, cascade.receiver_center_relative_radius.w, distance);
}

fn lunar_map_coverage(p: vec3<f32>) -> vec2<f32> {
    if shadow.flags.y == 2u {
        return vec2<f32>(shadow_coverage(shadow,p),shadow_coverage(landscape_shadow,p));
    }
    return vec2<f32>(shadow_coverage(secondary_moon,p),shadow_coverage(secondary_landscape,p));
}
