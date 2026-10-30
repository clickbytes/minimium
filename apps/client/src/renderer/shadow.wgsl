struct ShadowUniform {
    light_view_projection: mat4x4<f32>,
    receiver_center_relative_radius: vec4<f32>,
    texel_depth_range: vec4<f32>,
    bias_fade: vec4<f32>,
    flags: vec4<u32>,
};

@group(0) @binding(0) var<uniform> shadow: ShadowUniform;

struct ShadowVertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
};

struct ShadowVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex fn vs_shadow(input: ShadowVertexInput) -> ShadowVertexOutput {
    var output: ShadowVertexOutput;
    output.clip_position = shadow.light_view_projection * vec4<f32>(input.position, 1.0);
    return output;
}
