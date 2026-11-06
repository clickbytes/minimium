struct TerrainSettings {
    scales_a: vec4<f32>,
    scales_b_blend: vec4<f32>,
    cover: vec4<f32>,
    debug: vec4<u32>,
    cover_center: vec4<f32>,
    cover_east: vec4<f32>,
    cover_north: vec4<f32>,
    eye_relative: vec4<f32>,
    climate: vec4<f32>,
    mean_albedo: array<vec4<f32>,6>,
};
@group(1) @binding(0) var material_albedo: texture_2d_array<f32>;
@group(1) @binding(1) var material_normal: texture_2d_array<f32>;
@group(1) @binding(2) var material_packed: texture_2d_array<f32>;
@group(1) @binding(3) var material_sampler: sampler;
@group(1) @binding(4) var<uniform> terrain: TerrainSettings;
@group(1) @binding(5) var surface_cover: texture_2d_array<f32>;
@group(1) @binding(6) var planet_cover: texture_2d_array<f32>;

struct MaterialSample {
    albedo: vec3<f32>,
    normal: vec3<f32>,
    packed: vec4<f32>,
};

fn material_scale(layer: u32) -> f32 {
    if layer < 4u { return terrain.scales_a[layer]; }
    return terrain.scales_b_blend[layer - 4u];
}

fn tile_hash(p: vec2<f32>) -> vec2<f32> {
    var h = bitcast<vec2<u32>>(vec2<i32>(p));
    h = h ^ (h.yx*1664525u+1013904223u);
    h = h ^ (h >> vec2<u32>(16u));
    h *= 2246822519u;
    h = h ^ (h >> vec2<u32>(13u));
    return vec2<f32>(h & vec2<u32>(0x00ffffffu))/16777216.0;
}

// Blending independent offsets on a coarse lattice avoids hard random-tile edges.
struct WarpedCoordinates {
    uv: vec2<f32>,
    du: vec2<f32>,
    dv: vec2<f32>,
};

fn warped_uv(high: vec2<f32>, low: vec2<f32>, scale: f32) -> WarpedCoordinates {
    let anchor_cell = floor(high/64.0);
    let local = (high-anchor_cell*64.0+low)/64.0;
    let cell = anchor_cell+floor(local);
    let fraction = fract(local);
    let t = fraction*fraction*(3.0-2.0*fraction);
    let dt = 6.0*fraction*(1.0-fraction)/64.0;
    let a = tile_hash(cell);
    let b = tile_hash(cell + vec2<f32>(1.0, 0.0));
    let c = tile_hash(cell + vec2<f32>(0.0, 1.0));
    let d = tile_hash(cell + vec2<f32>(1.0));
    let warp = mix(mix(a, b, t.x), mix(c, d, t.x), t.y);
    let du = vec2<f32>(1.0/scale, 0.0) + 2.0*mix(b-a, d-c, t.y)*dt.x;
    let dv = vec2<f32>(0.0, 1.0/scale) + 2.0*mix(c-a, d-b, t.x)*dt.y;
    // Integer modular arithmetic preserves millimetre tile phase on Earth-sized planets.
    let period = max(i32(round(scale*1000.0)), 1);
    let anchor = vec2<i32>(high);
    let phase = vec2<f32>((anchor % vec2<i32>(period))*1000 % vec2<i32>(period))/f32(period);
    return WarpedCoordinates(phase + low / (f32(period)/1000.0) + warp * 2.0, du, dv);
}

// Gradients are evaluated once in the fragment entry, before material selection.
fn plane_sample(high: vec2<f32>, low: vec2<f32>, layer: u32, dx: vec2<f32>, dy: vec2<f32>, detail: f32) -> MaterialSample {
    let scale = material_scale(layer);
    let coordinates = warped_uv(high, low, scale);
    let uv = coordinates.uv;
    let gradient_x = coordinates.du*dx.x + coordinates.dv*dx.y;
    let gradient_y = coordinates.du*dy.x + coordinates.dv*dy.y;
    let color = textureSampleGrad(material_albedo, material_sampler, uv, i32(layer), gradient_x, gradient_y).rgb;
    let packed = textureSampleGrad(material_packed, material_sampler, uv, i32(layer), gradient_x, gradient_y);
    var normal = vec3<f32>(0.0, 0.0, 1.0);
    if detail > 0.001 {
        let sampled = textureSampleGrad(material_normal, material_sampler, uv, i32(layer), gradient_x, gradient_y).xy*2.0-1.0;
        let texels = f32(textureDimensions(material_normal).x);
        let footprint = max(length(gradient_x),length(gradient_y))*texels;
        let resolved = 1.0-smoothstep(1.0,8.0,footprint);
        let xy = vec2<f32>(dot(coordinates.du*scale, sampled), dot(coordinates.dv*scale, sampled))*detail*resolved;
        normal = vec3<f32>(xy, sqrt(max(1.0-dot(xy, xy), 0.001)));
    }
    return MaterialSample(color, normal, packed);
}

fn material_resolution(layer: u32, dx: vec3<f32>, dy: vec3<f32>) -> f32 {
    let footprint = max(length(dx),length(dy))/material_scale(layer);
    return 1.0-smoothstep(0.08,0.5,footprint);
}

fn terrain_material(high: vec3<f32>, low: vec3<f32>, n: vec3<f32>, layer: u32,
    dx: vec3<f32>, dy: vec3<f32>, detail: f32) -> MaterialSample {
    let p = abs(n)*abs(n)*abs(n)*abs(n);
    let raw_w = p/max(dot(p, vec3<f32>(1.0)), 0.0001);
    // A skipped projection must have zero contribution at the branch boundary.
    let w = raw_w*smoothstep(vec3<f32>(0.02),vec3<f32>(0.04),raw_w);
    var color = vec3<f32>(0.0);
    var packed = vec4<f32>(0.0);
    var gradient = vec3<f32>(0.0);
    var total = 0.0;
    for (var axis = 0u; axis < 3u; axis++) {
        if w[axis] > 0.0 {
            var sample: MaterialSample;
            var g = vec3<f32>(0.0);
            if axis == 0u {
                sample = plane_sample(high.yz, low.yz, layer, dx.yz, dy.yz, detail);
                g = vec3<f32>(0.0, sample.normal.x, sample.normal.y);
            } else if axis == 1u {
                sample = plane_sample(high.xz, low.xz, layer, dx.xz, dy.xz, detail);
                g = vec3<f32>(sample.normal.x, 0.0, sample.normal.y);
            } else {
                sample = plane_sample(high.xy, low.xy, layer, dx.xy, dy.xy, detail);
                g = vec3<f32>(sample.normal.x, sample.normal.y, 0.0);
            }
            color += sample.albedo*w[axis];
            packed += sample.packed*w[axis];
            gradient += g*w[axis];
            total += w[axis];
        }
    }
    var result = MaterialSample(color/total, normalize(n+gradient-n*dot(n, gradient)), packed/total);
    return result;
}

fn surface_light(n: vec3<f32>, v: vec3<f32>, l: vec3<f32>, albedo: vec3<f32>, roughness: f32, f0: f32) -> vec3<f32> {
    let half_vector = v+l;
    let h = half_vector/max(length(half_vector),0.00001);
    let nl = max(dot(n,l), 0.0);
    let nv = max(dot(n,v), 0.001);
    let nh = max(dot(n,h), 0.0);
    let vh = max(dot(v,h), 0.0);
    let a2 = pow(roughness, 4.0);
    let d = a2 / max(3.14159265 * pow(nh*nh*(a2-1.0)+1.0, 2.0), 0.0001);
    let k = pow(roughness+1.0, 2.0)/8.0;
    let g = nl/(nl*(1.0-k)+k) * nv/(nv*(1.0-k)+k);
    let fresnel = f0 + (1.0-f0)*pow(1.0-vh, 5.0);
    return albedo*(1.0-fresnel)*nl + vec3<f32>(d*g*fresnel/max(4.0*nv, 0.001));
}

// Low-cost hemisphere lighting and a broad sky reflection. This has no scene
// captures, visibility queries or sharp environment reflections.
fn terrain_ambient(n: vec3<f32>, v: vec3<f32>, up: vec3<f32>, albedo: vec3<f32>,
    roughness: f32, ao: f32, f0: f32, local_position: vec3<f32>) -> vec3<f32> {
    var ambient_light=environment.ambient_rgb_exposure.xyz;
    if physical_atmosphere() { ambient_light=physical_ambient(local_position); }
    let sky_facing = clamp(dot(n,up)*0.5+0.5,0.0,1.0);
    let diffuse_light = ambient_light*mix(0.45,1.0,sky_facing);
    let reflection = reflect(-v,n);
    let sky_weight = clamp(dot(reflection,up),0.0,1.0);
    var sky = mix(environment.sky_horizon_rgb_star_visibility.xyz,
        environment.sky_zenith_rgb_sun_altitude.xyz,sqrt(sky_weight));
    if physical_atmosphere() {sky=ambient_light*mix(1.4,2.2,sqrt(sky_weight));}
    let ground = ambient_light*albedo*0.35;
    let reflected_light = mix(ground,sky,smoothstep(-0.1,0.25,dot(reflection,up)));
    let nv = max(dot(n,v),0.0);
    let fresnel = f0+(max(1.0-roughness,f0)-f0)*pow(1.0-nv,5.0);
    // Broad lobes retain less directional contrast; keep the approximation weak.
    let specular = reflected_light*fresnel*mix(0.45,0.15,roughness);
    return (albedo*diffuse_light*(1.0-fresnel)+specular)*ao;
}
