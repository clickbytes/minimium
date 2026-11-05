struct MoonSphereVertex {
    @builtin(position) clip_position: vec4<f32>,
};

struct MoonSphereFragment {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> MoonSphereVertex {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let offset = environment.moon_orbit_world.xyz - camera.eye_world.xyz;
    let distance = length(offset);
    let direction = offset / max(distance, 0.000001);
    let angular_radius = asin(clamp(environment.moon_orbit_world.w / distance, 0.0, 0.999));
    let reference = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(direction.y) > 0.95);
    let right = normalize(cross(reference, direction));
    let up = normalize(cross(direction, right));
    let ray = normalize(direction + (right * corners[index].x + up * corners[index].y) * tan(angular_radius));
    let clip = camera.sky_view_projection * vec4<f32>(ray, 0.0);
    var output: MoonSphereVertex;
    if clip.w <= 0.00001 {
        output.clip_position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    } else {
        output.clip_position = vec4<f32>(clip.xy, clip.w, clip.w);
    }
    return output;
}

@fragment fn fs_main(input: MoonSphereVertex) -> MoonSphereFragment {
    let ndc = vec2<f32>(
        input.clip_position.x * camera.viewport.z * 2.0 - 1.0,
        1.0 - input.clip_position.y * camera.viewport.w * 2.0,
    );
    let far_point = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray = normalize(far_point.xyz / max(abs(far_point.w), 0.000001));
    let center = environment.moon_orbit_world.xyz;
    let radius = environment.moon_orbit_world.w;
    let to_center = center - camera.eye_world.xyz;
    let along_ray = dot(to_center, ray);
    let closest_offset = to_center - ray * along_ray;
    let discriminant = radius * radius - dot(closest_offset, closest_offset);
    if discriminant <= 0.0 {
        discard;
    }
    let distance = along_ray - sqrt(discriminant);
    if distance <= 0.0 {
        discard;
    }
    let hit = camera.eye_world.xyz + ray * distance;
    let normal = normalize(hit - center);
    let sun = normalize(environment.moon_light_world_phase.xyz);
    let illumination = max(dot(normal, sun), 0.0);
    let albedo = vec3<f32>(0.72, 0.73, 0.75);
    let linear = albedo * (0.015 + 1.8 * illumination);
    let projected = camera.view_projection * vec4<f32>(hit, 1.0);
    if projected.w <= 0.0 {
        discard;
    }
    var output: MoonSphereFragment;
    output.color = vec4<f32>(tone_map_linear(linear), 1.0);
    output.depth = clamp(projected.z / projected.w, 0.0, 1.0);
    return output;
}
