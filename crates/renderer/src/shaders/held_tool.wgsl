struct VertexInput {
    @location(0) view_position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let distance = max(-input.view_position.z, 0.1);
    output.clip_position = vec4<f32>(
        input.view_position.xy * 1.25 / distance,
        0.5,
        1.0,
    );
    output.color = input.color;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, 1.0);
}

// Opaque viewmodel after all world blending and the metering pass. Signed
// alpha identifies a display-calibrated pixel for the final HDR resolve.
@fragment
fn fs_hdr_overlay(input: VertexOutput) -> @location(0) vec4<f32> {
    return vec4<f32>(input.color, -1.0);
}
