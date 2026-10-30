struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var globe: texture_2d_array<f32>;
@group(1) @binding(1) var globe_sampler: sampler;

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
    let east = sample_map(input.uv + vec2<f32>(offset.x, 0.0)).a;
    let west = sample_map(input.uv - vec2<f32>(offset.x, 0.0)).a;
    let north = sample_map(input.uv - vec2<f32>(0.0, offset.y)).a;
    let south = sample_map(input.uv + vec2<f32>(0.0, offset.y)).a;
    let slope = vec3<f32>((west-east) * 6.0, (north-south) * 6.0, 1.0);
    let relief = 0.7 + 0.3 * max(dot(normalize(slope), normalize(vec3<f32>(0.5, 0.7, 1.0))), 0.0);
    let daylight = 0.43 + 0.57 * max(dot(input.normal, normalize(vec3<f32>(0.6, 0.7, 0.35))), 0.0);
    return vec4<f32>(sample.rgb * relief * daylight, 1.0);
}
