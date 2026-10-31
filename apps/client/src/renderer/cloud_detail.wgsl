// CPU counterpart: environment/weather/clouds/detail.rs.
// Deterministic lobe-local 3D noise. Never hash camera position or frame number.
fn cloud_hash(cell: vec3<i32>, seed: u32) -> f32 {
    let key = bitcast<vec3<u32>>(cell);
    var value = key.x * 0x8da6b343u ^ key.y * 0xd8163841u
        ^ key.z * 0xcb1ab31fu ^ seed;
    value = (value ^ (value >> 16u)) * 0x7feb352du;
    value = (value ^ (value >> 15u)) * 0x846ca68bu;
    value ^= value >> 16u;
    return f32(value >> 8u) / 16777216.0;
}

fn cloud_noise(p: vec3<f32>, seed: u32) -> f32 {
    let base = vec3<i32>(floor(p));
    let t = fract(p);
    let w = t * t * (vec3<f32>(3.0) - t * 2.0);
    var value = 0.0;
    for (var z = 0; z < 2; z++) {
        for (var y = 0; y < 2; y++) {
            for (var x = 0; x < 2; x++) {
                let weight = select(1.0 - w.x, w.x, x == 1)
                    * select(1.0 - w.y, w.y, y == 1)
                    * select(1.0 - w.z, w.z, z == 1);
                value += cloud_hash(base + vec3<i32>(x, y, z), seed) * weight;
            }
        }
    }
    return value;
}

fn cloud_kernel(q: vec3<f32>, seed: u32) -> f32 {
    let radial = 1.0 - dot(q, q);
    if radial <= 0.0 { return 0.0; }
    let coarse = cloud_noise(q * 3.5 + vec3<f32>(7.13, 13.7, 19.2), seed);
    let fine = cloud_noise(q * 11.0 + vec3<f32>(37.0, 11.0, 3.0), seed ^ 0x9e3779b9u);
    let carved = max(radial + (coarse - 0.5) * 0.26 + (fine - 0.5) * 0.08, 0.0);
    let edge = smoothstep(0.0, 0.035, radial);
    return carved * carved * edge * (0.72 + 0.56 * fine);
}
