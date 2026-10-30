fn cloud_layer(
    ray: vec3<f32>, east: vec3<f32>, north: vec3<f32>, up: vec3<f32>,
    cover: f32, scale: f32, height: f32, phase: vec2<f32>,
) -> f32 {
    let altitude = dot(ray, up);
    let projected = vec2<f32>(dot(ray, east), dot(ray, north))
        * height / max(altitude, 0.06);
    let uv = projected * scale + phase;
    let broad = textureSample(weather_noise, weather_sampler, uv).r;
    let detail = textureSample(weather_noise, weather_sampler, uv * 2.91 + vec2<f32>(0.37, 0.71)).r;
    let density = broad * 0.76 + detail * 0.24;
    return smoothstep(1.0 - cover - 0.11, 1.0 - cover + 0.11, density)
        * smoothstep(0.002, 0.015, altitude) * step(0.005, cover);
}

@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(
        position.x * camera.viewport.z * 2.0 - 1.0,
        1.0 - position.y * camera.viewport.w * 2.0,
    );
    let far_point = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray = normalize(far_point.xyz / max(abs(far_point.w), 1.0e-6));
    let up = normalize(environment.observer_up_horizon_cos.xyz);
    let east = normalize(cross(vec3<f32>(0.0, 1.0, 0.001), up));
    let north = normalize(cross(up, east));
    let time = environment.star_animation.x;
    let drift = weather.wind.xy * time * 0.000003;
    let high = cloud_layer(ray, east, north, up, weather.cover_rain.z,
        0.032, 1.0, drift * 0.4 + vec2<f32>(0.14, 0.31));
    let middle = cloud_layer(ray, east, north, up, weather.cover_rain.y,
        0.054, 1.0, drift * 0.7 + vec2<f32>(0.47, 0.11));
    let low = cloud_layer(ray, east, north, up, weather.cover_rain.x,
        0.087, 1.0, drift + vec2<f32>(0.71, 0.63));
    let coverage = 1.0 - (1.0 - high * 0.54) * (1.0 - middle * 0.72) * (1.0 - low * 0.90);
    let visibility = horizon_visibility(ray);
    let alpha = coverage * visibility;
    if alpha < 0.001 {
        return vec4<f32>(0.0);
    }
    let sun_altitude = environment.sky_zenith_rgb_sun_altitude.w;
    let daylight = smoothstep(radians(-9.0), radians(5.0), sun_altitude);
    let sunward = max(dot(ray, normalize(environment.sun_apparent_world.xyz)), 0.0);
    let moonlight = clamp(environment.local_moon_rgb_visibility.w * 4.0, 0.0, 1.0);
    let base_night = vec3<f32>(0.012, 0.017, 0.027) * (1.0 + moonlight);
    let base_day = mix(vec3<f32>(0.42, 0.48, 0.54), vec3<f32>(0.84, 0.85, 0.86),
        smoothstep(0.0, 0.8, sunward));
    let sunset = exp(-abs(sun_altitude) * 11.0) * sunward;
    let color = tone_map_linear(mix(base_night, base_day, daylight)
        + vec3<f32>(0.34, 0.13, 0.04) * sunset * daylight);
    return vec4<f32>(color * alpha, alpha);
}
