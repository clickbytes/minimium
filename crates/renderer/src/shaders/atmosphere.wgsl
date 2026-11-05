@vertex fn vs_main(@builtin(vertex_index) vertex: u32) -> @builtin(position) vec4<f32> {
    let points = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(points[vertex], 0.0, 1.0);
}

@group(2) @binding(0) var scene_depth: texture_depth_2d;

@fragment fn fs_atmosphere(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = min(vec2<i32>(position.xy), vec2<i32>(textureDimensions(scene_depth)) - 1);
    if atmosphere.config.x <= 0.0 || environment.flags.z == 0u
        || textureLoad(scene_depth, pixel, 0) < 0.999999 { discard; }
    let ndc = vec2<f32>(position.x / camera.viewport.x * 2.0 - 1.0,
        1.0 - position.y / camera.viewport.y * 2.0);
    let direction = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray = normalize(direction.xyz / max(abs(direction.w), 1e-6));
    let air = air_path(atmosphere_table, atmosphere_sampler, camera.eye_world.xyz, ray,
        atmosphere.config.x * 100.0, atmosphere.config.x, select(4u, 8u, atmosphere.config.y > 0.5), atmosphere.config.z);
    let opacity = 1.0 - dot(air.transmission, vec3<f32>(0.2126, 0.7152, 0.0722));
    return vec4<f32>(tone_map_linear(air.scattering), opacity);
}
