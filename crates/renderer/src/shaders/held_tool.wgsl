struct VertexInput {
    @location(0) view_position: vec3<f32>,
    @location(1) color: vec3<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
};

struct HeldToolUniform {
    transform: mat4x4<f32>,
    projection: vec4<f32>,
};

@group(0) @binding(0) var<uniform> tool: HeldToolUniform;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let position = tool.transform * vec4<f32>(input.view_position, 1.0);
    output.clip_position = vec4<f32>(
        position.x * tool.projection.x,
        position.y * tool.projection.y,
        position.z * tool.projection.z + tool.projection.w,
        -position.z,
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
