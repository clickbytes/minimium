@group(0) @binding(0) var accumulation: texture_2d<f32>;
@group(0) @binding(1) var revealage: texture_2d<f32>;
struct Vertex {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> Vertex {
    let p = array<vec2<f32>,3>(vec2(-1.0,-1.0), vec2(3.0,-1.0), vec2(-1.0,3.0))[index];
    return Vertex(vec4(p, 0.0, 1.0), p * vec2(0.5,-0.5) + vec2(0.5));
}
@fragment fn fs_main(input: Vertex) -> @location(0) vec4<f32> {
    let size = textureDimensions(accumulation);
    let pixel = clamp(vec2<i32>(input.uv * vec2<f32>(size)), vec2(0), vec2<i32>(size) - vec2(1));
    let sum = textureLoad(accumulation, pixel, 0);
    let reveal = textureLoad(revealage, pixel, 0).r;
    return vec4(sum.rgb / max(sum.a, 0.00001), 1.0 - clamp(reveal, 0.0, 1.0));
}
