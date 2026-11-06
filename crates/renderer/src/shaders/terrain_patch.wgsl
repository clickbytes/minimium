// Stateless, planet-space value noise. No sector IDs, time or render origin.
fn patch_hash(cell: vec3<i32>, salt: u32) -> f32 {
    let c = bitcast<vec3<u32>>(cell);
    var h = c.x*1597334677u ^ c.y*3812015801u ^ c.z*2798796415u;
    h ^= terrain.debug.y ^ terrain.debug.z*2246822519u ^ salt;
    h = (h ^ (h >> 16u))*2246822519u;
    h = (h ^ (h >> 13u))*3266489917u;
    return f32((h ^ (h >> 16u)) & 0x00ffffffu)/16777216.0;
}

fn patch_noise(high: vec3<f32>, low: vec3<f32>, scale: f32, salt: u32) -> f32 {
    // Split before adding: preserve local precision at Earth-sized coordinates.
    let anchor = floor(high/scale);
    let local = (high-anchor*scale+low)/scale;
    let cell = vec3<i32>(anchor)+vec3<i32>(floor(local));
    let f = fract(local);
    let t = f*f*f*(f*(f*6.0-15.0)+10.0);
    let a = mix(patch_hash(cell, salt), patch_hash(cell+vec3<i32>(1,0,0), salt), t.x);
    let b = mix(patch_hash(cell+vec3<i32>(0,1,0), salt), patch_hash(cell+vec3<i32>(1,1,0), salt), t.x);
    let c = mix(patch_hash(cell+vec3<i32>(0,0,1), salt), patch_hash(cell+vec3<i32>(1,0,1), salt), t.x);
    let d = mix(patch_hash(cell+vec3<i32>(0,1,1), salt), patch_hash(cell+vec3<i32>(1,1,1), salt), t.x);
    return mix(mix(a,b,t.y), mix(c,d,t.y), t.z);
}

fn patch_fields(high: vec3<f32>, low: vec3<f32>, footprint: f32) -> vec3<f32> {
    var broad = 0.5;
    var ground = 0.5;
    var tuft = 0.5;
    if (footprint < 2048.0) { broad = patch_noise(high, low, 2048.0, 1327u); }
    if (footprint < 32.0) { ground = patch_noise(high, low, 32.0, 4789u); }
    if (footprint < 4.0) { tuft = patch_noise(high, low, 4.0, 7919u); }
    // Remove unresolved variation rather than aliasing it into distant stripes.
    // Interpolation concentrates raw values near 0.5. Remap before filtering
    // so actual patches survive instead of a nearly uniform material average.
    let shaped = smoothstep(vec3<f32>(0.2),vec3<f32>(0.8),vec3<f32>(broad,ground,tuft));
    return mix(shaped, vec3<f32>(0.5),
        smoothstep(vec3<f32>(512.0,8.0,1.0), vec3<f32>(2048.0,32.0,4.0), vec3<f32>(footprint)));
}

// Measured landform dominates habitat; noise only perturbs its local boundary.
fn habitat_moisture(fields: vec3<f32>, moisture: f32, wetness: f32, retention: f32) -> f32 {
    return clamp(moisture*(0.65+retention*0.85)+wetness*0.12+(fields.x-0.5)*0.04,0.0,1.0);
}

fn patch_coverage(base: array<f32,6>, fields: vec3<f32>, vegetation: f32,
    thermal: f32, habitat: f32, slope: f32, footprint: f32) -> array<f32,6> {
    var weights = base;
    let support = thermal*smoothstep(0.10,0.36,habitat)*(1.0-smoothstep(0.08,0.3,slope));
    let frequency = mix(0.08,0.72,smoothstep(0.1,0.55,habitat));
    let threshold = 1.0-frequency;
    let resolved = 1.0-smoothstep(2.0,24.0,footprint);
    // Integrate unresolved patches towards their area fraction, not a threshold
    // applied to mean noise (which would make entire biomes disappear at a LOD).
    let local = smoothstep(threshold-0.16,threshold+0.16,fields.y*0.7+fields.z*0.3);
    let patchiness = mix(frequency,local,resolved);
    let loose = base[0]+base[1];
    let colonized = support*patchiness*0.45;
    let dry = 1.0-smoothstep(0.16,0.5,habitat);
    let sand_fraction = clamp(base[0]/max(loose,0.0001)+dry*(fields.y-0.5)*0.25,0.0,1.0);
    weights[0] = loose*(1.0-colonized)*sand_fraction;
    weights[1] = loose*(1.0-colonized)*(1.0-sand_fraction);
    let living = clamp(vegetation*mix(0.45,1.55,patchiness),0.0,1.0);
    weights[2] = base[2]*living+loose*colonized;
    weights[1] += base[2]*(1.0-living);
    weights[3] = base[3]*mix(1.15,0.8,patchiness);
    var total = 0.0;
    for (var i = 0u; i < 6u; i++) { total += weights[i]; }
    for (var i = 0u; i < 6u; i++) { weights[i] /= max(total,0.0001); }
    return weights;
}
