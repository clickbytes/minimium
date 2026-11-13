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
};
@vertex fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_projection * vec4<f32>(input.position, 1.0);
    out.color = input.color;
    out.normal = input.normal;
    out.world_position = input.position;
    return out;
}
@fragment fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let normal = normalize(input.normal + vec3<f32>(0.0, 1.0e-8, 0.0));
    let albedo = srgb_to_linear(input.color);
    var direct = vec3<f32>(0.0);
    let sun_direction = select(environment.sun_geometric_world.xyz,environment.global_sun_direction.xyz,physical_atmosphere());
    if length(sun_direction) > 1.0e-8 {
        let sun_diffuse = max(dot(normal, normalize(sun_direction)), 0.0);
        var visibility = 1.0;
        if shadow.flags.x != 0u && shadow.flags.y == 1u {
            visibility = shadow_visibility(input.world_position, normal, sun_direction);
        }
        var irradiance=environment.local_sun_rgb_visibility.xyz;
        if physical_atmosphere() { irradiance=physical_sun(input.world_position); }
        direct = irradiance * sun_diffuse
            * visibility;
    }
    let moon_direction = select(environment.moon_geometric_world.xyz,surface_moon_direction(),physical_atmosphere());
    if length(moon_direction) > 1.0e-8 {
        let moon_diffuse = max(dot(normal, normalize(moon_direction)), 0.0);
        var visibility = 1.0;
        if shadow.flags.x != 0u && shadow.flags.y == 2u {
            visibility = shadow_visibility(input.world_position, normal, moon_direction);
        }
        var irradiance=environment.local_moon_rgb_visibility.xyz;
        if physical_atmosphere() {irradiance=physical_moon(input.world_position);}
        direct += irradiance * moon_diffuse * visibility;
    }
    var ambient=environment.ambient_rgb_exposure.xyz;
    if physical_atmosphere() { ambient=physical_ambient(input.world_position); }
    var bolid_visibility = 1.0;
    if shadow.flags.y == 3u {
        bolid_visibility = shadow_visibility(input.world_position, normal,
            environment.bolid_position_intensity.xyz - (input.world_position + environment.render_origin.xyz));
    }
    direct += bolid_irradiance(input.world_position, normal) * bolid_visibility;
    direct += transient_irradiance(input.world_position, normal);
    if transient_lights.count.y!=0u {return transient_debug_transmission(input.world_position);}
    let linear = albedo * (ambient + direct);
    return vec4<f32>(tone_map_linear(linear), 1.0);
}
