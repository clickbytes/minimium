struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec3<f32>,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
};
@group(0) @binding(0) var font_atlas: texture_2d<f32>;
@group(0) @binding(1) var font_sampler: sampler;
const surface_is_srgb: bool = OUTPUT_SURFACE_IS_SRGB;
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(input.position, 0.0, 1.0);
    out.uv = input.uv;
    out.color = input.color;
    return out;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let alpha = textureSample(font_atlas, font_sampler, input.uv).r;
    let linear = srgb_to_linear(input.color);
    let encoded = select(linear_to_srgb(linear), linear, surface_is_srgb);
    return vec4<f32>(encoded, alpha);
}
