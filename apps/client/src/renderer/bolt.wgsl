struct BoltVertex {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) fade: f32,
};

fn bolt_point(index: u32, up: vec3<f32>, east: vec3<f32>, north: vec3<f32>) -> vec3<f32> {
    let height = 900.0 + f32(index) * 1050.0;
    let drift = sin(f32(index) * 19.3 + weather.event.x * 63.0) * 350.0;
    let branch = cos(f32(index) * 11.7 + weather.event.x * 47.0) * 220.0;
    return up * weather.blend.y + (up * height + east * drift + north * branch) * weather.wind.z;
}

@vertex fn vs_bolt(@builtin(vertex_index) vertex: u32,
    @builtin(instance_index) segment: u32) -> BoltVertex {
    let up = normalize(weather.lightning.xyz);
    let reference = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0),
        abs(up.y) > 0.95);
    let east = normalize(cross(reference, up));
    let north = normalize(cross(up, east));
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-0.5, 0.0), vec2<f32>(0.5, 0.0), vec2<f32>(-0.5, 1.0),
        vec2<f32>(-0.5, 1.0), vec2<f32>(0.5, 0.0), vec2<f32>(0.5, 1.0));
    let corner = corners[vertex];
    let start = bolt_point(segment, up, east, north);
    let end = bolt_point(segment + 1u, up, east, north);
    let point = mix(start, end, corner.y) + east * corner.x * 32.0 * weather.wind.z;
    var output: BoltVertex;
    output.clip_position = camera.view_projection * vec4<f32>(point - weather.origin.xyz, 1.0);
    output.fade = weather.lightning.w * (1.0 - f32(segment) / 12.0);
    return output;
}

@fragment fn fs_bolt(input: BoltVertex) -> @location(0) vec4<f32> {
    let alpha = clamp(input.fade, 0.0, 1.0);
    return vec4<f32>(vec3<f32>(1.3, 1.5, 1.8) * alpha, alpha);
}
