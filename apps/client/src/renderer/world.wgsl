struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
};
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_projection * vec4<f32>(input.position, 1.0);
    out.color = input.color;
    out.normal = input.normal;
    return out;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let light = normalize(vec3<f32>(0.46, 0.82, 0.34));
    let diffuse = max(dot(normalize(input.normal), light), 0.0);
    let water = input.color.b > input.color.r * 1.5 && input.color.b > input.color.g * 1.25;
    let intensity = select(0.28 + 0.72 * diffuse, 0.68 + 0.22 * diffuse, water);
    return vec4<f32>(input.color * intensity, 1.0);
}
