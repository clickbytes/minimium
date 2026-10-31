struct WorldWeatherUniform {
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
    atmosphere_radii: vec4<f32>,
    atmosphere_rayleigh: vec4<f32>,
    atmosphere_mie: vec4<f32>,
    atmosphere_ozone: vec4<f32>,
};
@group(3) @binding(0) var<uniform> world_weather: WorldWeatherUniform;
@group(3) @binding(1) var world_weather_atlas: texture_2d_array<f32>;
@group(3) @binding(2) var world_weather_sampler: sampler;

fn cloud_sun_visibility(surface: vec3<f32>, sun: vec3<f32>) -> f32 {
    return cached_cloud_shadow(cloud_shadow_atlas, world_weather_sampler, surface, sun,
        world_weather.event.y, world_weather.blend.y, world_weather.cloud_region);
}

@group(3) @binding(3) var cloud_shadow_atlas: texture_2d_array<f32>;
