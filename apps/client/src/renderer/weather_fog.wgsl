@group(2) @binding(0) var scene_depth: texture_depth_2d;

fn fog_sphere_length(eye: vec3<f32>, ray: vec3<f32>, limit: f32, radius: f32) -> f32 {
    let b = dot(eye, ray);
    let discriminant = b * b - dot(eye, eye) + radius * radius;
    if discriminant <= 0.0 { return 0.0; }
    let root = sqrt(discriminant);
    return max(min(limit, -b + root) - max(0.0, -b - root), 0.0);
}

// A single spatial sample and analytic shell length approximate distant rain.
// The cache carries cloud-hosted precipitation, never the regional rain potential.
fn rain_curtain(ray: vec3<f32>, distance: f32, ground: f32) -> f32 {
    let point = camera.eye_world.xyz + ray * (distance * 0.5);
    let cloud = cached_cloud_weather(cloud_shadow_atlas, weather_sampler, point,
        weather.event.y, weather.cloud_region);
    if cloud.a < 0.001 { return 0.0; }
    let channel = select(8, 6, cloud.r > 0.08);
    let layer = weather_channel(weather_atlas, weather_sampler, point, weather.blend.x,
        weather.region, channel);
    let radius = weather.blend.y;
    let scale = max(weather.wind.z, 0.0001);
    let eye = camera.eye_world.xyz / radius;
    let top = 1.0 + max(layer.x * 20000.0, ground) * scale / radius;
    let bottom = 1.0 + ground * scale / radius;
    let path = max(fog_sphere_length(eye, ray, distance / radius, top)
        - fog_sphere_length(eye, ray, distance / radius, bottom), 0.0) * radius / scale;
    return path * 0.00006 * pow(cloud.a * 32.0, 0.7);
}

// One analytic height-fog integral; no cloud density integration in screen space.
@fragment fn fs_fog(input: WeatherVertexOutput) -> @location(0) vec4<f32> {
    if weather.animation.z > 0.5 || weather.blend.y <= 0.0 { discard; }
    let uv = input.clip_position.xy / camera.viewport.xy;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let far = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray = normalize(far.xyz / far.w);
    let depth = textureLoad(scene_depth, vec2<i32>(input.clip_position.xy), 0);
    var distance = 30000.0 * weather.wind.z;
    if depth < 0.999999 {
        let hit = camera.inverse_view_projection * vec4<f32>(ndc, depth, 1.0);
        distance = min(distance, length(hit.xyz / hit.w + weather.origin.xyz - camera.eye_world.xyz));
    }
    let up = normalize(camera.eye_world.xyz);
    let auxiliary = weather_auxiliary(weather_atlas, weather_sampler, up, weather.blend.x, weather.region);
    let dynamics = weather_dynamics(weather_atlas, weather_sampler, up, weather.blend.x, weather.region);
    let scale = max(weather.wind.z, 0.0001);
    let height = max((length(camera.eye_world.xyz) - weather.blend.y) / scale - dynamics.w * 20000.0, 0.0);
    let physical_distance = distance / scale;
    let falloff = 90.0 + auxiliary.z * 130.0;
    let slope = dot(up, ray) / falloff;
    let integral = select(physical_distance, (1.0 - exp(clamp(-slope * physical_distance, -60.0, 5.0)))
        / select(0.000001, slope, abs(slope) > 0.000001), abs(slope) > 0.000001);
    let variation = 0.55 + 0.45 * weather_noise(up * 1700.0);
    let fog_optical = auxiliary.z * variation * exp(-height / falloff) * max(integral, 0.0) * 0.0018;
    let rain_optical = rain_curtain(ray, distance, dynamics.w * 20000.0);
    let optical = fog_optical + rain_optical;
    let alpha = clamp(1.0 - exp(-optical), 0.0, 0.98);
    let fog_color = environment.sky_horizon_rgb_star_visibility.xyz * 0.6
        + environment.ambient_rgb_exposure.xyz * 0.25;
    let rain_color = environment.sky_horizon_rgb_star_visibility.xyz * 0.32
        + environment.ambient_rgb_exposure.xyz * 0.18;
    let color = (fog_color * fog_optical + rain_color * rain_optical) / max(optical, 0.000001);
    return vec4<f32>(tone_map_linear(color) * alpha, alpha);
}
