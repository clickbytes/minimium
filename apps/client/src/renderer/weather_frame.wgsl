struct WeatherUniform {
    cover_rain: vec4<f32>,
    wind: vec4<f32>,
};
@group(1) @binding(0) var<uniform> weather: WeatherUniform;
@group(1) @binding(1) var weather_noise: texture_2d<f32>;
@group(1) @binding(2) var weather_sampler: sampler;

struct WeatherVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> WeatherVertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var output: WeatherVertexOutput;
    output.clip_position = vec4<f32>(positions[index], 1.0, 1.0);
    return output;
}
