struct InstanceOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) @interpolate(flat) index: u32,
};
@vertex fn vs_cloud(@builtin(vertex_index) vertex: u32, @builtin(instance_index) index: u32) -> InstanceOutput {
    let lobe = lobe_state(cloud_instances[index]);
    let radius = max(lobe.axes.x, max(lobe.axes.y, lobe.axes.z));
    var low = vec2<f32>(1.0);
    var high = vec2<f32>(-1.0);
    var crosses_eye = false;
    for (var corner = 0u; corner < 8u; corner++) {
        let sign = vec3<f32>(f32(corner & 1u), f32((corner >> 1u) & 1u), f32((corner >> 2u) & 1u)) * 2.0 - 1.0;
        let offset = lobe.center - cloud_frame.eye.xyz + sign * radius;
        let clip = camera.sky_view_projection * vec4<f32>(offset, 0.0);
        if clip.w <= 0.000001 { crosses_eye = true; }
        let ndc = clip.xy / max(clip.w, 0.000001);
        low = min(low, ndc);
        high = max(high, ndc);
    }
    if crosses_eye { low = vec2<f32>(-1.0); high = vec2<f32>(1.0); }
    low = max(low, vec2<f32>(-1.0));
    high = min(high, vec2<f32>(1.0));
    let corners = array<vec2<f32>, 6>(vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0));
    return InstanceOutput(vec4<f32>(mix(low, high, corners[vertex]), 0.5, 1.0), index);
}
fn light_depth(lobe: Lobe, instance: CloudInstanceGpu, point: vec3<f32>, sun: vec3<f32>) -> f32 {
    let interval = lobe_interval(lobe, point, sun);
    var optical = lobe_shadow_integral(lobe, sun, interval, 0.0, interval.y);
    for (var n = 0u; n < u32(cloud_frame.config.z); n++) {
        var index = instance.neighbors_a[n % 4u];
        if n >= 4u { index = instance.neighbors_b[n % 4u]; }
        if index >= u32(cloud_frame.viewport.w) { continue; }
        let other = lobe_state(cloud_instances[index]);
        let hit = lobe_interval(other, point, sun);
        optical += lobe_shadow_integral(other, sun, hit, 0.0, hit.y);
    }
    return optical;
}
@fragment fn fs_cloud(input: InstanceOutput) -> CloudLayers {
    let instance = cloud_instances[input.index];
    let fade = select(cloud_frame.config.x, 1.0 - cloud_frame.config.x, instance.material.w < 0.0);
    if fade <= 0.0 { discard; }
    let lobe = lobe_state(instance);
    let view = cloud_ray(input.clip.xy);
    let interval = lobe_interval(lobe, cloud_frame.eye.xyz, view.ray);
    let lo = max(interval.x, view.bounds.x);
    let hi = min(interval.y, view.bounds.y);
    if hi <= lo { discard; }
    let point = cloud_frame.eye.xyz + view.ray * mix(lo, hi, 0.25);
    let world = point + cloud_frame.origin.xyz;
    let weight = local_weight(world) * fade;
    let local = inverse_rotate(lobe.rotation, point - lobe.center) / (lobe.axes * lobe.axes);
    let normal = normalize(rotate(lobe.rotation, local));
    let optical = lobe_integral(lobe, cloud_frame.eye.xyz, view.ray, interval, lo, hi);
    let sun = normalize(environment.global_sun_direction.xyz);
    let shadow = light_depth(lobe, instance, point, sun);
    var color = tone_map_linear(cloud_radiance(world, view.ray, normal, optical, shadow, u32(instance.material.x)));
    let debug = u32(cloud_frame.config.w);
    if debug == 2u { color = 0.5 + 0.5 * cos(vec3<f32>(0.0, 2.0, 4.0) + instance.material.y * 100.0); }
    if debug == 3u { color = vec3<f32>(1.0 - exp(-optical), exp(-optical), 0.0); }
    if debug == 4u { color = vec3<f32>(instance.material.z / 4.0, 0.3, 1.0 - instance.material.z / 4.0); }
    var values: array<vec4<f32>, 4>;
    for (var bin = 0u; bin < u32(cloud_frame.config.y); bin++) {
        var tau = lobe_integral(lobe, cloud_frame.eye.xyz, view.ray, interval, max(lo, bin_edge(view.bounds, bin)), min(hi, bin_edge(view.bounds, bin + 1u))) * weight;
        if debug == 1u { tau = select(0.0, 0.2 * weight, interval.w < 0.08); color = vec3<f32>(0.2, 1.0, 0.3); }
        values[bin] = vec4<f32>(color * tau, tau) * cloud_frame.eye.w;
    }
    return pack_bins(values);
}
