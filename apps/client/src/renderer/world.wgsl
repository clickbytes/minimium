struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec3<f32>,
    @location(2) normal: vec3<f32>,
};
struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) world_position: vec3<f32>,
    @location(3) fog_distance: f32,
};
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_projection * vec4<f32>(input.position, 1.0);
    out.color = input.color;
    out.normal = input.normal;
    out.world_position = input.position;
    out.fog_distance = max(out.clip_position.w, 0.0);
    return out;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(input.normal + vec3<f32>(0.0, 1.0e-8, 0.0));
    let radial_up = normalize(camera.eye_world.xyz + vec3<f32>(0.0, 0.00001, 0.0));
    let world_up = select(vec3<f32>(0.0, 1.0, 0.0), radial_up,
        length(camera.eye_world.xyz) > 1000.0);
    let upward = max(dot(normal, world_up), 0.0);
    let moisture = environment.weather_fog.z * smoothstep(0.35, 0.85, upward);
    let snow = moisture * environment.weather_fog.w;
    let wetness = moisture * (1.0 - environment.weather_fog.w);
    let albedo = mix(srgb_to_linear(input.color) * (1.0 - wetness * 0.34),
        vec3<f32>(0.72, 0.77, 0.83), snow * 0.65);
    var direct = vec3<f32>(0.0);
    let sun_direction = environment.sun_geometric_world.xyz;
    if length(sun_direction) > 1.0e-8 {
        let sun_diffuse = max(dot(normal, normalize(sun_direction)), 0.0);
        var visibility = 1.0;
        if shadow.flags.x != 0u && shadow.flags.y == 1u {
            visibility = shadow_visibility(input.world_position, normal, sun_direction);
        }
        let cloud_visibility = cloud_sun_visibility(
            input.world_position + world_weather.origin.xyz, normalize(sun_direction));
        direct = environment.local_sun_rgb_visibility.xyz * sun_diffuse
            * visibility * cloud_visibility;
    }
    let moon_direction = environment.moon_geometric_world.xyz;
    if length(moon_direction) > 1.0e-8 {
        let moon_diffuse = max(dot(normal, normalize(moon_direction)), 0.0);
        var visibility = 1.0;
        if shadow.flags.x != 0u && shadow.flags.y == 2u {
            visibility = shadow_visibility(input.world_position, normal, moon_direction);
        }
        let cloud_visibility = cloud_sun_visibility(
            input.world_position + world_weather.origin.xyz, normalize(moon_direction));
        direct += environment.local_moon_rgb_visibility.xyz * moon_diffuse * visibility * cloud_visibility;
    }
    let linear = albedo * (environment.ambient_rgb_exposure.xyz + direct);
    // Spatial fog is composited after opaque depth is available.
    return vec4<f32>(tone_map_linear(linear), 1.0);
}
