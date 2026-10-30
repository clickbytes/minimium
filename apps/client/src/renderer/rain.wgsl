@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let intensity = clamp(weather.cover_rain.w / 8.0, 0.0, 1.0);
    if intensity <= 0.001 {
        return vec4<f32>(0.0);
    }
    let time = environment.star_animation.x;
    let wind = weather.wind.x * 0.014;
    let x = position.x + position.y * wind;
    let cell = vec2<f32>(floor(x / 13.0), floor((position.y + time * 430.0) / 90.0));
    let seed = fract(sin(dot(cell, vec2<f32>(127.1, 311.7))) * 43758.5453);
    let column = fract(x / 13.0);
    let fall = fract((position.y + time * (360.0 + seed * 240.0)) / 90.0);
    let streak = 1.0 - smoothstep(0.0, 0.09, abs(column - seed));
    let length_mask = 1.0 - smoothstep(0.0, 0.3, fall);
    let alpha = streak * length_mask * intensity * 0.32;
    let daylight = smoothstep(radians(-10.0), radians(4.0),
        environment.sky_zenith_rgb_sun_altitude.w);
    let color = mix(vec3<f32>(0.28, 0.34, 0.42), vec3<f32>(0.67, 0.74, 0.79), daylight);
    return vec4<f32>(color * alpha, alpha);
}
