struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};
@group(0) @binding(0) var linear_image: texture_2d<f32>;
struct OutputParams { exposure: f32, hdr: f32, surface_is_srgb: f32, padding: f32 };
@group(0) @binding(1) var<uniform> params: OutputParams;

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[index], 0.0, 1.0);
    return output;
}

@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let linear = textureLoad(linear_image, pixel, 0);
    var color = linear.rgb;
    if params.hdr > 0.5 {
        color = tone_map_color(color * params.exposure);
    }
    if params.surface_is_srgb < 0.5 {
        color = linear_to_srgb(color);
    }
    return vec4<f32>(color, linear.a);
}
