@group(0) @binding(0) var emission: texture_2d<f32>;
@group(0) @binding(1) var transmission: texture_2d<f32>;
@group(0) @binding(2) var foreground: texture_2d<f32>;
@group(0) @binding(3) var dust: texture_2d<f32>;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs_main(@builtin(vertex_index) index:u32) -> Vertex {
    let p=array<vec2<f32>,3>(vec2(-1.0,-1.0),vec2(3.0,-1.0),vec2(-1.0,3.0))[index];
    return Vertex(vec4(p,0.0,1.0),p*vec2(0.5,-0.5)+0.5);
}
fn resolve(input:Vertex, hdr:bool)->vec4<f32> {
    let size=vec2<i32>(textureDimensions(emission));
    let pixel=clamp(vec2<i32>(input.uv*vec2<f32>(size)),vec2(0),size-1);
    let layer=textureLoad(emission,pixel,0);
    let scattered=textureLoad(dust,pixel,0);
    let t=textureLoad(transmission,pixel,0).r;
    let cloud=textureLoad(foreground,pixel,0);
    let front=cloud.rgb/max(cloud.a,0.00001)*(1.0-t);
    // B=front+t*background. Dust affects background, leaving front intact:
    // B*(1-alpha) + front*alpha + t*layer.rgb.
    var transmitted=layer.rgb*t*(1.0-scattered.a);
    if !hdr { transmitted=tone_map_color(transmitted); }
    return vec4(transmitted+scattered.rgb*t+front*scattered.a,scattered.a);
}
@fragment fn fs_main(input:Vertex)->@location(0) vec4<f32> { return resolve(input,true); }
@fragment fn fs_fallback(input:Vertex)->@location(0) vec4<f32> { return resolve(input,false); }
