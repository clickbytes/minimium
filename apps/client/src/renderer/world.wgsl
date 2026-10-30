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
    let normal = normalize(input.normal + vec3<f32>(0.0, 1.0e-8, 0.0));
    let albedo = srgb_to_linear(input.color);
    var direct = vec3<f32>(0.0);
    let sun_direction = environment.sun_geometric_world.xyz;
    if length(sun_direction) > 1.0e-8 {
        let sun_diffuse = max(dot(normal, normalize(sun_direction)), 0.0);
        direct = environment.local_sun_rgb_visibility.xyz * sun_diffuse;
    }
    let linear = albedo * (environment.ambient_rgb_exposure.xyz + direct);
    return vec4<f32>(tone_map_linear(linear * environment.ambient_rgb_exposure.w), 1.0);
}
