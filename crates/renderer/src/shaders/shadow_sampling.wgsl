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

// Orthographic shadow projection has orthogonal scaled rows. Inverting each
// row gives world displacement per UV/depth unit; intersect with the triangle
// plane analytically. No subtraction of nearly equal screen-space depth values.
fn receiver_depth_gradient(normal: vec3<f32>) -> vec2<f32> {
    let m = shadow.light_view_projection;
    let u = vec3<f32>(m[0][0],m[1][0],m[2][0])*0.5;
    let v = vec3<f32>(m[0][1],m[1][1],m[2][1])*(-0.5);
    let z = vec3<f32>(m[0][2],m[1][2],m[2][2]);
    // The plane becomes ill-conditioned as it approaches a light-tangent
    // plane. Fade the correction out smoothly instead of switching it off at
    // one threshold, which made tiny normal changes produce depth jumps.
    let normal_length_squared = dot(normal, normal);
    let z_length_squared = dot(z, z);
    let u_length_squared = dot(u, u);
    let v_length_squared = dot(v, v);
    if normal_length_squared <= 1e-20 || z_length_squared <= 1e-20
        || u_length_squared <= 1e-20 || v_length_squared <= 1e-20 {
        return vec2<f32>(0.0);
    }
    let light_facing = abs(dot(normal * inverseSqrt(normal_length_squared),
        z * inverseSqrt(z_length_squared)));
    let stability = smoothstep(0.005, 0.05, light_facing);
    if light_facing <= 0.005 {
        return vec2<f32>(0.0);
    }
    let nz = dot(normal,z)/max(dot(z,z),1e-20);
    if abs(nz) <= 1e-20 {
        return vec2<f32>(0.0);
    }
    let gradient = -vec2<f32>(dot(normal,u)/max(dot(u,u),1e-20),
        dot(normal,v)/max(dot(v,v),1e-20))/nz;
    return gradient * stability;
}

// Perspective projection maps a receiver plane to a plane in NDC. Project
// two world tangents with the local quotient-rule Jacobian, then solve its
// depth slope. PCF taps must compare the plane at their own texel centers.
fn point_receiver_depth_gradient(normal: vec3<f32>, clip: vec4<f32>) -> vec2<f32> {
    let n2 = dot(normal, normal);
    if n2 <= 1e-20 { return vec2<f32>(0.0); }
    let n = normal * inverseSqrt(n2);
    let axis = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), abs(n.x) > 0.8);
    let tangent = normalize(cross(n, axis));
    let bitangent = cross(n, tangent);
    let a = shadow.light_view_projection * vec4<f32>(tangent, 0.0);
    let b = shadow.light_view_projection * vec4<f32>(bitangent, 0.0);
    let ndc = clip.xyz / clip.w;
    let da = (a.xyz - ndc * a.w) / clip.w;
    let db = (b.xyz - ndc * b.w) / clip.w;
    let plane = cross(da, db);
    let scale = length(plane);
    if scale <= 1e-20 || abs(plane.z) <= scale * 0.00001 {
        return vec2<f32>(0.0);
    }
    return vec2<f32>(-2.0 * plane.x, 2.0 * plane.y) / plane.z;
}

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

    let projection_position = select(world_position, receiver_offset, shadow.flags.y == 3u);
    let clip = shadow.light_view_projection * vec4<f32>(projection_position, 1.0);
    if clip.w <= 0.000001 {
        return 1.0;
    }
    let ndc = clip.xyz / clip.w;
    if ndc.x < -1.0 || ndc.x > 1.0 || ndc.y < -1.0 || ndc.y > 1.0 || ndc.z < 0.0 || ndc.z > 1.0 {
        return 1.0;
    }

    var uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let normal_length_squared = dot(normal, normal);
    let light_length_squared = dot(light_direction, light_direction);
    let slope = select(1.0,
        1.0 - clamp(dot(normal * inverseSqrt(max(normal_length_squared, 1e-20)),
            light_direction * inverseSqrt(max(light_length_squared, 1e-20))), 0.0, 1.0),
        normal_length_squared > 1e-20 && light_length_squared > 1e-20);
    var bias_m = min(shadow.bias_fade.z, shadow.bias_fade.x + shadow.bias_fade.y * slope);
    if shadow.flags.y == 3u {
        // PCF taps may land on a neighboring facet, where extrapolating this
        // receiver plane is insufficient. Bound the separation by half a
        // receiver texel and the existing maximum world-space bias.
        let texel_m = 2.02 * receiver_radius * shadow.texel_depth_range.x;
        bias_m = min(shadow.bias_fade.z, max(bias_m, texel_m * 0.5));
    }
    let depth_range_m = max(shadow.texel_depth_range.w - shadow.texel_depth_range.z, 0.000001);
    var reference_depth = ndc.z - bias_m / depth_range_m;
    var depth_gradient = receiver_depth_gradient(normal);
    if shadow.flags.y == 3u {
        // Offset the receiver out of its surface, not merely along a grazing
        // light ray. Project the displaced point for both UV and depth so the
        // finite source still samples one consistent ray. The displacement is
        // bounded by the same world-space maximum used above.
        var offset = normalize(light_direction) * bias_m;
        if normal_length_squared > 1e-20 {
            let n = normal * inverseSqrt(normal_length_squared);
            offset = n * select(-bias_m, bias_m, dot(n, light_direction) >= 0.0);
        }
        let biased = shadow.light_view_projection * vec4<f32>(
            projection_position + offset,1.0);
        reference_depth = biased.z/biased.w;
        uv = vec2<f32>(biased.x/biased.w*0.5+0.5, 0.5-biased.y/biased.w*0.5);
        depth_gradient = point_receiver_depth_gradient(normal, biased);
    }
    let dimensions = vec2<i32>(textureDimensions(shadow_depth));
    let receiver_texel = vec2<i32>(floor(uv * vec2<f32>(dimensions)));
    var pcf_sum = 0.0;
    var sample_count = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let sample_texel = receiver_texel + vec2<i32>(x, y);
            // Continuous tent weights: entering/leaving taps have zero weight.
            let delta = abs(vec2<f32>(sample_texel)+0.5-uv*vec2<f32>(dimensions));
            let tent = max(vec2<f32>(1.5)-delta,vec2<f32>(0.0));
            let weight = tent.x*tent.y;
            if any(sample_texel < vec2<i32>(0)) || any(sample_texel >= dimensions) {
                pcf_sum += weight;
            } else {
                // Compare against the same texel center where rasterization stored depth.
                // Correcting at an unsnapped UV leaves a sawtooth error inside every texel.
                let center_uv = (vec2<f32>(sample_texel) + 0.5) / vec2<f32>(dimensions);
                let plane_depth = reference_depth + dot(depth_gradient, center_uv - uv);
                let stored_depth = textureLoad(shadow_depth, sample_texel, 0);
                pcf_sum += select(0.0, weight, plane_depth <= stored_depth);
            }
            sample_count += weight;
        }
    }

    let pcf_visibility = pcf_sum / max(sample_count, 1.0);
    let fade_start = min(shadow.bias_fade.w, receiver_radius);
    let fade = 1.0 - smoothstep(fade_start, receiver_radius, receiver_distance);
    var strength = 1.0;
    if shadow.flags.y == 3u {
        strength = clamp(bitcast<f32>(shadow.flags.z), 0.0, 1.0);
    }
    return mix(1.0, pcf_visibility, fade * strength);
}
