// One filtered depth comparison per cascade. Air samples have no receiver plane.
fn air_cascade_visibility(layer: i32, cascade: ShadowUniform, position: vec3<f32>) -> f32 {
    let clip = cascade.light_view_projection * vec4<f32>(position, 1.0);
    let ndc = clip.xyz / clip.w;
    if any(abs(ndc.xy) > vec2<f32>(1.0)) || ndc.z < 0.0 || ndc.z > 1.0 {
        return 1.0;
    }
    let uv = ndc.xy * vec2<f32>(0.5, -0.5) + 0.5;
    let depth_span = max(cascade.texel_depth_range.w - cascade.texel_depth_range.z, 0.001);
    return textureSampleCompareLevel(shadow_depth, shadow_sampler, uv, layer, ndc.z - cascade.bias_fade.x/depth_span);
}

fn air_shadow_visibility(position: vec3<f32>) -> f32 {
    if shadow.flags.x == 0u || (shadow.flags.y != 1u && shadow.flags.y != 2u) {
        return 1.0;
    }
    let near_weight = shadow_coverage(shadow, position);
    var far_visibility = 1.0;
    if near_weight < 1.0 && landscape_shadow.flags.x != 0u && landscape_shadow.flags.y == shadow.flags.y {
        let weight = shadow_coverage(landscape_shadow, position);
        if weight > 0.0 {
            far_visibility = mix(1.0, air_cascade_visibility(1, landscape_shadow, position), weight);
        }
    }
    if near_weight == 0.0 { return far_visibility; }
    return mix(far_visibility, air_cascade_visibility(0, shadow, position), near_weight);
}
