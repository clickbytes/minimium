// Coarse inter-layer shadows use the same occupied-area cache as ground shadows.
// Height checks matter: a convective low cloud can extend ABOVE the middle deck.
fn cloud_upper_visibility(point: vec3<f32>, sun: vec3<f32>, own_layer: u32) -> f32 {
    let up = normalize(point);
    let height = (length(point) - weather.blend.y) / weather.wind.z;
    let cosine = dot(up, sun);
    if cosine < -0.03 { return 1.0; }
    let heights = cached_cloud_channel(cloud_shadow_atlas, weather_sampler, up,
        weather.event.y, weather.cloud_region, 4).rgb * 20000.0;
    var transmission = 1.0;
    for (var layer = 0u; layer < 3u; layer++) {
        if layer == own_layer { continue; }
        let top = heights[layer];
        let above = smoothstep(height - 150.0, height + 350.0, top);
        if above < 0.001 { continue; }
        let r = weather.blend.y + max(height + 50.0, top) * weather.wind.z;
        let b = dot(point, sun);
        let distance = -b + sqrt(max(b * b - dot(point, point) + r * r, 0.0));
        let opacity = cached_cloud_opacity(cloud_shadow_atlas, weather_sampler,
            point + sun * distance, weather.event.y, weather.cloud_region)[layer];
        transmission *= pow(max(1.0 - opacity * above, 0.01), 1.0 / max(cosine, 0.25));
    }
    return transmission;
}

// Bounded Henyey-Greenstein-like forward lobe. This replaces a broad
// arbitrary power curve while avoiding unbounded silver-lining highlights.
fn cloud_phase(cos_theta: f32, g: f32) -> f32 {
    let den = max(1.0 + g * g - 2.0 * g * clamp(cos_theta, -1.0, 1.0), 0.015);
    return (1.0 - g * g) / pow(den, 1.5);
}

fn cloud_radiance(point: vec3<f32>, ray: vec3<f32>, normal: vec3<f32>,
    optical: f32, light_optical: f32, layer: u32) -> vec3<f32> {
    let up = normalize(point);
    let sun = normalize(environment.global_sun_direction.xyz);
    let solar_cosine = dot(up, sun);
    let top = clamp(dot(normal, up) * 0.5 + 0.5, 0.0, 1.0);
    let solar = air_light(atmosphere_table, weather_sampler, point, sun, weather.blend.y);
    let upper = cloud_upper_visibility(point, sun, layer);
    let self_transmission = exp(-light_optical);
    let wrapped = clamp((dot(normal, sun) + 0.35) / 1.35, 0.0, 1.0);
    // Strong forward scattering is most visible at thin cloud edges near the Sun.
    // High ice clouds scatter more strongly forward than the water-cloud fallback.
    let anisotropy = select(0.72, 0.82, layer == 2u);
    let thin_edge = exp(-max(optical, 0.0) * 0.16);
    let forward = min(cloud_phase(dot(ray, sun), anisotropy) * 0.015, 0.48)
        * thin_edge;
    // Lit storm tops stay white. Dense bases and sides darken through extinction,
    // not a black storm tint. Thin ice decks transmit more sky light.
    let direct = environment.global_sun_rgb.xyz * solar * upper
        * (wrapped * (0.12 + 0.88 * self_transmission) + forward);
    let twilight = smoothstep(-0.12, 0.08, solar_cosine);
    let ambient = vec3<f32>(0.24, 0.33, 0.49) * twilight
        * (0.025 + 0.16 * top) * (0.18 + 0.82 * exp(-optical * 0.22))
        * (0.35 + 0.65 * upper);
    let moon_dir = global_moon_direction(point);
    let moon = global_moon_irradiance()
        * air_light(atmosphere_table, weather_sampler, point, moon_dir, weather.blend.y)
        * max(dot(normal, moon_dir) * 0.5 + 0.5, 0.0) * (0.2 + 0.8 * self_transmission);
    let night_fill = weather.presentation.x * weather.presentation.z
        * vec3<f32>(0.75, 0.85, 1.0) * (0.3 + 0.7 * top)
        * (0.35 + 0.65 * exp(-optical * 0.12)) * (1.0 - twilight);
    let flash = weather.lightning.w * exp(-pow(acos(clamp(dot(up, weather.lightning.xyz), -1.0, 1.0)) / 0.012, 2.0));
    var color = direct + ambient + moon + night_fill + vec3<f32>(0.48, 0.58, 0.8) * flash;
    if weather.animation.z > 0.5 {
        color = air_surface(atmosphere_table, weather_sampler, point, color, weather.blend.y,
            select(2u, 4u, weather.blend.w > 0.5), weather.presentation.x);
    } else {
        let distance = length(point - camera.eye_world.xyz);
        let haze = (1.0 - exp(-distance / max(weather.wind.z * 110000.0, 1.0)))
            * (1.0 - smoothstep(0.0, 0.6, abs(dot(ray, normalize(camera.eye_world.xyz)))));
        color = mix(color, environment.sky_horizon_rgb_star_visibility.xyz, haze * 0.65);
    }
    return color;
}
