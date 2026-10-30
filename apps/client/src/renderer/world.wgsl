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
    let water = input.color.b > input.color.r * 1.5 && input.color.b > input.color.g * 1.25;
    let normal = normalize(input.normal);
    if environment.flags.z == 1u {
        if environment.flags.x == 0u {
            return vec4<f32>(input.color * environment.ambient_rgb_exposure.xyz, 1.0);
        }
        let diffuse = max(dot(normal, normalize(environment.sun_geometric_world.xyz)), 0.0);
        let direct = environment.local_sun_rgb_visibility.xyz * diffuse;
        let ambient = environment.ambient_rgb_exposure.xyz;
        let water_scale = select(1.0, 0.85, water);
        return vec4<f32>(input.color * (ambient + direct) * water_scale, 1.0);
    }
    let light = normalize(vec3<f32>(0.46, 0.82, 0.34));
    let diffuse = max(dot(normal, light), 0.0);
    let intensity = select(0.28 + 0.72 * diffuse, 0.68 + 0.22 * diffuse, water);
    return vec4<f32>(input.color * intensity, 1.0);
}
