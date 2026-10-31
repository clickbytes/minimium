// Curved distant moments of the same physical field as the analytic instances.
struct CloudInput {
    @location(0) direction: vec3<f32>,
    @location(1) material_a: vec4<f32>,
    @location(2) material_b: vec4<f32>,
    @location(3) heights: vec4<f32>,
    @location(4) layer: f32,
};
struct CloudOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) point: vec3<f32>,
    @location(1) material: vec2<f32>,
    @location(2) height: f32,
    @location(3) @interpolate(flat) layer: u32,
};
@vertex fn vs_cloud(input: CloudInput) -> CloudOutput {
    let t = weather.event.y;
    let material = mix(input.material_a, input.material_b, t);
    let height = mix(input.heights.x + input.heights.y, input.heights.z + input.heights.w, t) * 0.5;
    let point = input.direction * (weather.blend.y + height * weather.wind.z);
    let clip = camera.sky_view_projection * vec4<f32>(point - camera.eye_world.xyz, 0.0);
    return CloudOutput(vec4<f32>(clip.xy, clip.w * 0.5, clip.w), point, material.xy, height, u32(input.layer));
}
@fragment fn fs_cloud(input: CloudOutput) -> CloudLayers {
    if input.material.x < 0.0001 { discard; }
    let view = cloud_ray(input.clip.xy);
    let eye = camera.eye_world.xyz / weather.blend.y;
    let b = dot(eye, view.ray);
    let closest = eye - view.ray * b;
    let radius = 1.0 + input.height / 6371000.0;
    let root = sqrt(max(radius * radius - dot(closest, closest), 0.0));
    let near = -b - root;
    let far = -b + root;
    let mesh = length(input.point - camera.eye_world.xyz) / weather.blend.y;
    let distance = select(far, near, near > 0.0 && abs(near - mesh) <= abs(far - mesh)) * weather.blend.y;
    if distance < view.bounds.x || distance > view.bounds.y { discard; }
    let point = camera.eye_world.xyz + view.ray * distance;
    let weight = 1.0 - local_weight(point);
    if weight < 0.0001 { discard; }
    let up = normalize(point);
    let optical = input.material.y / max(abs(dot(up, view.ray)), 0.2);
    // Preserve empty area in an aggregate; do not turn it into dilute overcast.
    let alpha = input.material.x * (1.0 - exp(-optical));
    let tau = -log(max(1.0 - alpha, 0.000001)) * weight;
    var color = tone_map_linear(cloud_radiance(point, view.ray, up, optical, optical * 0.15, input.layer));
    if u32(cloud_frame.config.w) == 4u { color = vec3<f32>(0.2, 0.4, 1.0); }
    var values: array<vec4<f32>, 4>;
    for (var bin = 0u; bin < u32(cloud_frame.config.y); bin++) {
        if distance >= bin_edge(view.bounds, bin) && distance <= bin_edge(view.bounds, bin + 1u) {
            values[bin] = vec4<f32>(color * tau, tau) * cloud_frame.eye.w;
            break;
        }
    }
    return pack_bins(values);
}
