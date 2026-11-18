struct VertexOutput {
    @builtin(position) position: vec4<f32>,
};
@group(0) @binding(0) var linear_image: texture_2d<f32>;
@group(0) @binding(1) var<uniform> params: OutputParams;
@group(0) @binding(2) var<uniform> eye: EyeExposure;

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> VertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var output: VertexOutput;
    output.position = vec4<f32>(positions[index], 0.0, 1.0);
    return output;
}

@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(position.xy);
    let linear = textureLoad(linear_image, pixel, 0);
    var color = linear.rgb;
    if params.hdr > 0.5 {
        let adapted = select(1.0,eye.multiplier,params.elapsed>=-1.0 && linear.a>=0.0);
        let scene_gain = select(params.exposure*adapted,params.fixed_gain,params.fixed_gain>0.0 && linear.a>=0.0);
        let gain = select(scene_gain,1.0,linear.a == -2.0);
        color = tone_map_color(color * gain);
    }
    let underwater = clamp(params.underwater, 0.0, 1.0);
    let underwater_color = color * vec3<f32>(0.48, 0.74, 0.9) + vec3<f32>(0.005, 0.025, 0.04);
    color = mix(color, underwater_color, underwater);
    if params.surface_is_srgb < 0.5 {
        color = linear_to_srgb(color);
    }
    return vec4<f32>(color, select(linear.a,1.0,linear.a<0.0));
}
