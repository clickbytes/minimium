struct MarkerInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
};
struct MarkerOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex fn vs_main(input: MarkerInput) -> MarkerOutput {
    var output: MarkerOutput;
    output.clip_position = camera.view_projection * vec4<f32>(input.position, 1.0);
    output.color = input.color;
    return output;
}

@fragment fn fs_main(input: MarkerOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(tone_map_linear(srgb_to_linear(input.color)), 1.0);
}
