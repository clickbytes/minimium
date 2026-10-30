struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};
@group(0) @binding(0) var linear_image: texture_2d<f32>;

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
    return vec4<f32>(linear_to_srgb(linear.rgb), linear.a);
}
