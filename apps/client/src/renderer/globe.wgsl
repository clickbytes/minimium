@group(1) @binding(0) var globe: texture_2d_array<f32>;
@group(1) @binding(1) var globe_sampler: sampler;
@group(2) @binding(0) var weather_atlas: texture_2d<f32>;
@group(2) @binding(1) var weather_sampler: sampler;
@group(2) @binding(2) var cloud_noise: texture_2d<f32>;

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
    let linear = sample.rgb * environment.global_sun_rgb.xyz * diffuse;
    let weather_uv = vec2<f32>(fract(input.uv.x), clamp(input.uv.y, 0.00390625, 0.99609375));
    let conditions = textureSample(weather_atlas, weather_sampler, weather_uv);
    let noise = textureSample(cloud_noise, weather_sampler,
        weather_uv * vec2<f32>(4.0, 2.0) + vec2<f32>(environment.star_animation.x * 0.00001, 0.0)).r;
    let coverage = max(conditions.r, max(conditions.g * 0.75, conditions.b * 0.55));
    let cloud = smoothstep(1.0 - coverage - 0.12, 1.0 - coverage + 0.12, noise);
    let rain_darkening = conditions.a * 0.18;
    let cloud_lit = vec3<f32>(0.75, 0.78, 0.82) *
        (0.035 + diffuse * 0.9) * environment.global_sun_rgb.xyz;
    let cloud_night = vec3<f32>(0.012, 0.016, 0.022);
    let cloud_color = max(cloud_lit, cloud_night) * (1.0 - rain_darkening);
    return vec4<f32>(tone_map_linear(mix(linear, cloud_color, cloud * 0.92)), 1.0);
}
