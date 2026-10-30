struct SkyVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> SkyVertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var output: SkyVertexOutput;
    output.clip_position = vec4<f32>(positions[index], 1.0, 1.0);
    return output;
}

@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(
        position.x * camera.viewport.z * 2.0 - 1.0,
        1.0 - position.y * camera.viewport.w * 2.0,
    );
    let far_point = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray_world = normalize(far_point.xyz / max(abs(far_point.w), 1.0e-6));
    let sample = sample_clear_sky(ray_world);
    let sky = tone_map_linear(sample.scattering_rgb);
    let ground = HORIZON_OCCLUSION_RGB;
    let visibility = horizon_visibility(ray_world);
    return vec4<f32>(mix(ground, sky, visibility), 1.0);
}
