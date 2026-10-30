struct SunVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> SunVertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[index];
    let sun = normalize(environment.sun_apparent_world.xyz);
    let reference = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(sun.y) > 0.95);
    let right = normalize(cross(reference, sun));
    let up = normalize(cross(sun, right));
    let radius = max(environment.sun_apparent_world.w, 0.0);
    let direction = normalize(sun + (right * corner.x + up * corner.y) * tan(radius));
    var output: SunVertexOutput;
    output.clip_position = camera.sky_view_projection * vec4<f32>(direction, 0.0);
    output.clip_position.z = output.clip_position.w;
    return output;
}

@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(
        position.x * camera.viewport.z * 2.0 - 1.0,
        1.0 - position.y * camera.viewport.w * 2.0,
    );
    let far_point = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray_world = normalize(far_point.xyz / max(abs(far_point.w), 1.0e-6));
    let sun = normalize(environment.sun_apparent_world.xyz);
    let angle = acos(clamp(dot(ray_world, sun), -1.0, 1.0));
    let radius = max(environment.sun_apparent_world.w, 1.0e-7);
    let edge = max(fwidth(angle), 1.0e-7);
    let coverage = 1.0 - smoothstep(radius - edge, radius + edge, angle);
    let visibility = horizon_visibility(ray_world);
    if coverage <= 0.0 || visibility <= 0.0 {
        discard;
    }
    let atmosphere = sample_clear_sky(ray_world);
    let solar_radiance = SOLAR_DISK_RADIANCE * atmosphere.transmittance;
    let halo = atmosphere.scattering_rgb * SOLAR_HALO_FACTOR;
    let color = tone_map_linear((solar_radiance + halo) * environment.ambient_rgb_exposure.w);
    return vec4<f32>(color, coverage * visibility);
}
