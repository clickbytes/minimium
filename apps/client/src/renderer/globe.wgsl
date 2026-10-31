@group(1) @binding(0) var globe: texture_2d_array<f32>;
@group(1) @binding(1) var globe_sampler: sampler;
@group(2) @binding(0) var weather_atlas: texture_2d_array<f32>;
@group(2) @binding(1) var weather_sampler: sampler;
struct GlobeWeatherUniform {
    cover_rain: vec4<f32>,
    wind: vec4<f32>,
    blend: vec4<f32>,
    origin: vec4<f32>,
    lightning: vec4<f32>,
    event: vec4<f32>,
    shelter: vec4<f32>,
    region: vec4<f32>,
    animation: vec4<f32>,
    cloud_region: vec4<f32>,
    presentation: vec4<f32>,
};
@group(2) @binding(2) var<uniform> weather: GlobeWeatherUniform;

fn sample_map(uv: vec2<f32>) -> vec4<f32> {
    let wrapped = vec2<f32>(fract(uv.x), clamp(uv.y, 0.0, 0.999999));
    let scaled = wrapped * 4.0;
    let tile = vec2<u32>(min(floor(scaled), vec2<f32>(3.0)));
    let local_uv = scaled - vec2<f32>(tile);
    let layer = i32(tile.y * 4u + tile.x);
    return textureSample(globe, globe_sampler, local_uv, layer);
}

struct Input {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};
struct Output {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) normal: vec3<f32>,
};
@vertex fn vs_main(input: Input) -> Output {
    var output: Output;
    output.clip_position = camera.view_projection * vec4<f32>(input.position, 1.0);
    output.uv = input.uv;
    output.normal = normalize(input.position);
    return output;
}
@fragment fn fs_main(input: Output) -> @location(0) vec4<f32> {
    let sample = sample_map(input.uv);
    let offset = vec2<f32>(1.0) / (vec2<f32>(textureDimensions(globe)) * 4.0);
    let east_height = sample_map(input.uv + vec2<f32>(offset.x, 0.0)).a;
    let west_height = sample_map(input.uv - vec2<f32>(offset.x, 0.0)).a;
    let north_height = sample_map(input.uv - vec2<f32>(0.0, offset.y)).a;
    let south_height = sample_map(input.uv + vec2<f32>(0.0, offset.y)).a;
    let latitude = (0.5 - input.uv.y) * 3.14159265;
    let longitude = (input.uv.x - 0.5) * 6.28318530;
    let up = normalize(input.normal);
    let east_dir = vec3<f32>(-sin(longitude), 0.0, cos(longitude));
    let north_dir = vec3<f32>(-sin(latitude) * cos(longitude), cos(latitude), -sin(latitude) * sin(longitude));
    var relief_normal = up;
    if abs(cos(latitude)) > 0.002 {
        let east_slope = (east_height - west_height) * 6.0;
        let north_slope = (north_height - south_height) * 6.0;
        relief_normal = normalize(up - east_slope * east_dir - north_slope * north_dir);
    }
    if environment.flags.z == 0u {
        let fallback = environment.ambient_rgb_exposure.xyz;
        return vec4<f32>(tone_map_linear(fallback), 1.0);
    }
    let sun_direction = normalize(environment.global_sun_direction.xyz);
    let diffuse = max(dot(relief_normal, sun_direction), 0.0);
    let cloud_visibility = cached_cloud_shadow(cloud_shadow_atlas, weather_sampler,
        up * weather.blend.y, sun_direction, weather.event.y,
        weather.blend.y, weather.cloud_region);
    let point = up * weather.blend.y;
    let transmission = air_light(atmosphere_table, weather_sampler, point, sun_direction, weather.blend.y);
    let moon_direction = global_moon_direction(point);
    let moon = global_moon_irradiance()
        * air_light(atmosphere_table, weather_sampler, point, moon_direction, weather.blend.y)
        * max(dot(relief_normal, moon_direction), 0.0);
    let day = smoothstep(-0.12, 0.08, dot(up, sun_direction));
    let map_fill = weather.presentation.x * weather.presentation.z
        * vec3<f32>(0.35, 0.45, 0.6) * (1.0 - day);
    let visibility = mix(cloud_visibility, max(cloud_visibility, 0.35), weather.presentation.x);
    let irradiance = environment.global_sun_rgb.xyz * transmission * diffuse * visibility
        + vec3<f32>(0.008, 0.012, 0.02) * day + moon + map_fill;
    let linear = sample.rgb * irradiance;
    let conditions = cached_cloud_weather(cloud_shadow_atlas, weather_sampler,
        input.normal, weather.event.y, weather.cloud_region);
    let auxiliary = weather_auxiliary(weather_atlas, weather_sampler,
        input.normal, weather.blend.x, weather.region);
    let flash = exp(-pow(acos(clamp(dot(normalize(input.normal),
        weather.lightning.xyz), -1.0, 1.0)) / 0.012, 2.0)) * weather.lightning.w;
    let wet = auxiliary.a * smoothstep(0.50, 0.54, auxiliary.g)
        * smoothstep(0.48, 0.52, sample.a);
    let ground = linear * (1.0 - conditions.a * 0.15)
        * (1.0 - wet * 0.18)
        + vec3<f32>(0.36, 0.42, 0.54) * flash;
    let snow = (1.0 - smoothstep(0.46, 0.52, auxiliary.g)) * auxiliary.a
        * smoothstep(0.48, 0.52, sample.a);
    let snow_color = irradiance * 0.78;
    let color = air_surface(atmosphere_table, weather_sampler, point,
        mix(ground, snow_color, snow * 0.42), weather.blend.y,
        select(4u, 8u, weather.blend.w > 0.5), weather.presentation.x);
    return vec4<f32>(tone_map_linear(color), 1.0);
}

@group(2) @binding(3) var cloud_shadow_atlas: texture_2d_array<f32>;
@group(2) @binding(4) var atmosphere_table: texture_2d<f32>;
