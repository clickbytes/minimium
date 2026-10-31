@group(2) @binding(0) var scene_depth: texture_depth_2d;
struct CloudLayers {
    @location(0) near: vec4<f32>,
    @location(1) middle: vec4<f32>,
    @location(2) far: vec4<f32>,
    @location(3) distant: vec4<f32>,
};
struct CloudRay { ray: vec3<f32>, bounds: vec2<f32>, };
fn cloud_ray(position: vec2<f32>) -> CloudRay {
    let size = max(ceil(cloud_frame.viewport.xy * cloud_frame.viewport.z), vec2<f32>(1.0));
    let uv = position / size;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let far = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray = normalize(far.xyz / far.w);
    let radius = weather.blend.y;
    let eye = camera.eye_world.xyz / radius;
    let b = dot(eye, ray);
    let nearest = eye - ray * b;
    let perpendicular = dot(nearest, nearest);
    let outer = 1.0 + 22000.0 / 6371000.0;
    let root = sqrt(max(outer * outer - perpendicular, 0.0));
    var lo = max(0.0, -b - root) * radius;
    var hi = (-b + root) * radius;
    if perpendicular < 1.0 {
        let ground = (-b - sqrt(1.0 - perpendicular)) * radius;
        if ground > 0.0 { hi = min(hi, ground); }
    }
    let pixel = vec2<i32>(clamp(uv * camera.viewport.xy, vec2<f32>(0.0), camera.viewport.xy - 1.0));
    let depth = textureLoad(scene_depth, pixel, 0);
    if depth < 0.999999 {
        let hit = camera.inverse_view_projection * vec4<f32>(ndc, depth, 1.0);
        hi = min(hi, length(hit.xyz / hit.w + weather.origin.xyz - camera.eye_world.xyz));
    }
    return CloudRay(ray, vec2<f32>(lo, hi));
}
fn bin_edge(bounds: vec2<f32>, index: u32) -> f32 {
    let t = f32(index) / cloud_frame.config.y;
    return mix(bounds.x, bounds.y, t * t);
}
fn local_weight(point: vec3<f32>) -> f32 {
    let radius = cloud_frame.local_region.w;
    if radius <= 0.0 { return 0.0; }
    let angle = acos(clamp(dot(normalize(point), cloud_frame.local_region.xyz), -1.0, 1.0));
    return 1.0 - smoothstep(radius * 0.7, radius, angle);
}
fn pack_bins(values: array<vec4<f32>, 4>) -> CloudLayers {
    return CloudLayers(values[0], values[1], values[2], values[3]);
}
