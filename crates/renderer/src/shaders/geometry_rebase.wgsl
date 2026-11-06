struct Parameters { delta: vec3<f32>, stride: u32 }
@group(0) @binding(0) var<storage,read_write> vertices: array<f32>;
@group(0) @binding(1) var<uniform> parameters: Parameters;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x*parameters.stride;
    if (id.x >= arrayLength(&vertices)/parameters.stride) { return; }
    vertices[i] += parameters.delta.x;
    vertices[i+1u] += parameters.delta.y;
    vertices[i+2u] += parameters.delta.z;
}
