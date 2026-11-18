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
    let sun = sunlight_at_surface(input.world_position);
    let sun_direction = sun.direction;
    if length(sun_direction) > 1.0e-8 {
        let sun_diffuse = max(dot(normal, normalize(sun_direction)), 0.0);
        var visibility = 1.0;
        if shadow.flags.x != 0u && shadow.flags.y == 1u {
            visibility = shadow_visibility(input.world_position, normal, normal, sun_direction);
        }
        direct = sun.irradiance * sun_diffuse
            * visibility;
    }
    let moon = moonlight_at_surface(input.world_position);
    let moon_direction = moon.direction;
    if length(moon_direction) > 1.0e-8 {
        let moon_diffuse = max(dot(normal, normalize(moon_direction)), 0.0);
        let visibility = lunar_shadow_visibility(input.world_position,normal,normal,moon_direction);
        direct += moon.irradiance * moon_diffuse * visibility;
    }
    let ambient = physical_ambient(input.world_position);
    var bolid_visibility = 1.0;
    if shadow.flags.y == 3u {
        bolid_visibility = shadow_visibility(input.world_position, normal, normal,
            environment.bolid_position_intensity.xyz - (input.world_position + environment.render_origin.xyz));
    }
    direct += bolid_irradiance(input.world_position, normal) * bolid_visibility;
    direct += transient_irradiance(input.world_position, normal);
    if transient_lights.count.y!=0u {return transient_debug_transmission(input.world_position);}
    let linear = albedo * (ambient + direct);
    return vec4<f32>(tone_map_linear(linear), 1.0);
}
