struct HaloVertex {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) body: u32,
};

fn halo_direction(body: u32) -> vec3<f32> {
    if body == 0u {
        if environment.flags.y == 1u {
            return normalize(environment.global_sun_direction.xyz);
        }
        return normalize(environment.sun_apparent_world.xyz);
    }
    if environment.flags.y == 1u {
        return normalize(environment.moon_orbit_world.xyz - camera.eye_world.xyz);
    }
    return normalize(environment.moon_apparent_world.xyz);
}

@vertex fn vs_main(@builtin(vertex_index) index: u32, @builtin(instance_index) body: u32) -> HaloVertex {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let center = halo_direction(body);
    let extent = select(radians(8.0), radians(4.0), body == 1u);
    let reference = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(center.y) > 0.95);
    let right = normalize(cross(reference, center));
    let up = normalize(cross(center, right));
    let ray = normalize(center + (right * corners[index].x + up * corners[index].y) * tan(extent));
    let clip = camera.sky_view_projection * vec4<f32>(ray, 0.0);
    var output: HaloVertex;
    output.body = body;
    if clip.w <= 0.00001 {
        output.clip_position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    } else {
        output.clip_position = vec4<f32>(clip.xy, clip.w, clip.w);
    }
    return output;
}

@fragment fn fs_main(input: HaloVertex) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(
        input.clip_position.x * camera.viewport.z * 2.0 - 1.0,
        1.0 - input.clip_position.y * camera.viewport.w * 2.0,
    );
    let far_point = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray = normalize(far_point.xyz / max(abs(far_point.w), 0.000001));
    let center = halo_direction(input.body);
    let angle = acos(clamp(dot(ray, center), -1.0, 1.0));
    let horizon = horizon_visibility(ray);
    var radiance = vec3<f32>(0.0);
    if input.body == 0u {
        let envelope = exp(-angle / radians(1.7))
            * (1.0 - smoothstep(radians(7.0), radians(8.0), angle));
        let visibility = select(environment.local_sun_rgb_visibility.w, 1.0, environment.flags.y == 1u);
        radiance = vec3<f32>(0.32, 0.15, 0.045) * envelope * visibility;
    } else {
        let phase = clamp(environment.moon_light_world_phase.w, 0.0, 1.0);
        let sun = normalize(environment.moon_light_world_phase.xyz);
        let tangent = sun - center * dot(sun, center);
        let limb = ray - center * dot(ray, center);
        let facing = dot(normalize(tangent + vec3<f32>(0.000001)), normalize(limb + vec3<f32>(0.000001)));
        let orientation = 1.0 + 0.45 * (1.0 - phase) * facing;
        let envelope = exp(-angle / radians(0.9))
            * (1.0 - smoothstep(radians(3.5), radians(4.0), angle));
        let visibility = select(environment.local_moon_rgb_visibility.w, 1.0, environment.flags.y == 1u);
        radiance = vec3<f32>(0.09, 0.12, 0.18) * envelope * pow(phase, 1.4)
            * orientation * visibility;
    }
    let color = tone_map_linear(radiance * horizon * environment.ambient_rgb_exposure.w);
    return vec4<f32>(color, 0.0);
}
