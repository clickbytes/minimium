struct Parameters { delta: vec3<f32>, stride: u32 }
@group(0) @binding(0) var<storage,read_write> vertices: array<f32>;
@group(0) @binding(1) var<uniform> parameters: Parameters;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if parameters.stride == 28u {
        // A terrain invocation owns the whole triangle. Rebasing rounds its
        // positions again, so the old plane normal can no longer be used for
        // receiver-plane shadow correction. No cross-invocation reads/writes.
        let first = id.x * 3u * parameters.stride;
        if first + 3u * parameters.stride > arrayLength(&vertices) { return; }
        var positions: array<vec3<f32>, 3>;
        for (var corner = 0u; corner < 3u; corner++) {
            let base = first + corner * parameters.stride;
            let p = vec3<f32>(vertices[base], vertices[base+1u], vertices[base+2u]) + parameters.delta;
            positions[corner] = p;
            vertices[base] = p.x;
            vertices[base+1u] = p.y;
            vertices[base+2u] = p.z;
        }
        let plane = cross(positions[1]-positions[0], positions[2]-positions[0]);
        let normal = plane * inverseSqrt(max(dot(plane, plane), 1e-30));
        for (var corner = 0u; corner < 3u; corner++) {
            let base = first + corner * parameters.stride + 22u;
            vertices[base] = normal.x;
            vertices[base+1u] = normal.y;
            vertices[base+2u] = normal.z;
        }
        return;
    }
    let i = id.x*parameters.stride;
    if (id.x >= arrayLength(&vertices)/parameters.stride) { return; }
    vertices[i] += parameters.delta.x;
    vertices[i+1u] += parameters.delta.y;
    vertices[i+2u] += parameters.delta.z;
}
