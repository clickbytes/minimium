struct CloudUniform {
    projection: mat4x4<f32>,
    scene_projection: mat4x4<f32>,
    inverse_projection: mat4x4<f32>,
    origin_radius: vec4<f32>,
    eye: vec4<f32>,
    observer: vec4<f32>,
    config: vec4<u32>,
};
struct CloudRecord {
    center_age: vec4<f32>,
    east_opacity: vec4<f32>,
    up_layer: vec4<f32>,
    north_region: vec4<f32>,
    morph: vec4<f32>,
};
@group(1) @binding(0) var<uniform> cloud_frame: CloudUniform;
@group(1) @binding(1) var<storage, read> clusters: array<CloudRecord>;
@group(1) @binding(2) var scene_depth: texture_depth_2d;
@group(2) @binding(0) var bolid_distance: texture_2d<f32>;

struct CloudVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) local: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) @interpolate(flat) parameters: vec4<f32>,
    @location(3) @interpolate(flat) detail: vec2<f32>,
};

fn basis(c: CloudRecord, p: vec3<f32>) -> vec3<f32> {
    return c.east_opacity.xyz * p.x + c.up_layer.xyz * p.y + c.north_region.xyz * p.z;
}

@vertex
fn vs_cloud(@builtin(vertex_index) index: u32, @location(0) center: vec4<f32>,
    @location(1) u: vec4<f32>, @location(2) v: vec4<f32>) -> CloudVertex {
    let corners = array<vec2<f32>, 6>(vec2(-1.0,-1.0), vec2(1.0,-1.0), vec2(1.0,1.0),
        vec2(-1.0,-1.0), vec2(1.0,1.0), vec2(-1.0,1.0));
    let c = clusters[u32(center.w)];
    let local = c.center_age.xyz + basis(c, center.xyz + u.xyz * corners[index].x + v.xyz * corners[index].y);
    var out: CloudVertex;
    out.position = cloud_frame.projection * vec4(local, 1.0);
    out.local = local;
    out.normal = basis(c, normalize(cross(u.xyz, v.xyz)));
    let phase = clamp((c.center_age.w - c.morph.x) / c.morph.y, 0.0, 1.0);
    let coverage = max(mix(u.w, v.w, phase), 0.0);
    out.parameters = vec4(c.center_age.w, c.east_opacity.w * coverage, c.up_layer.w, c.north_region.w);
    out.detail = c.morph.zw;
    return out;
}

fn hidden(local: vec3<f32>) -> bool {
    let clip = cloud_frame.scene_projection * vec4(local, 1.0);
    let ndc = clip.xyz / clip.w;
    let uv = ndc.xy * vec2(0.5, -0.5) + vec2(0.5);
    let size = textureDimensions(scene_depth);
    let pixel = clamp(vec2<i32>(uv * vec2<f32>(size)), vec2(0), vec2<i32>(size) - vec2(1));
    let depth = textureLoad(scene_depth, pixel, 0);
    if depth < 1.0 && ndc.z > depth + 0.0000001 { return true; }
    // Physical planet occlusion also works beyond the terrain's short far plane.
    let eye = cloud_frame.eye.xyz + cloud_frame.origin_radius.xyz;
    let delta = local - cloud_frame.eye.xyz;
    let distance = length(delta);
    let ray = delta / max(distance, 0.000001);
    let radius = cloud_frame.origin_radius.w;
    let eye_radius = length(eye);
    let b = dot(eye, ray);
    let discriminant = b * b - (eye_radius - radius) * (eye_radius + radius);
    if discriminant >= 0.0 && eye_radius >= radius {
        let hit = -b - sqrt(discriminant);
        if hit > 0.0 && hit < distance - max(radius * 0.0000001, 0.01) { return true; }
    }
    return false;
}

fn region_uv(direction: vec3<f32>) -> vec2<f32> {
    let a = abs(direction);
    if a.x >= a.y && a.x >= a.z { return direction.yz / a.x * 0.5 + vec2(0.5); }
    if a.y >= a.z { return direction.xz / a.y * 0.5 + vec2(0.5); }
    return direction.xy / a.z * 0.5 + vec2(0.5);
}

fn edge_fade(direction: vec3<f32>) -> f32 {
    let angle = acos(clamp(dot(direction, cloud_frame.observer.xyz), -1.0, 1.0));
    return 1.0 - smoothstep(cloud_frame.eye.w, cloud_frame.observer.w, angle);
}

// Reduced scattering on the existing voxel boundary, not a volume ray march.
// Both angular lobes integrate to one: opacity still supplies the intercepted
// fraction during composition, so do not multiply it into radiance again.
fn cloud_bolid_radiance(input: CloudVertex, normal: vec3<f32>) -> vec3<f32> {
    if environment.bolid_position_intensity.w <= 0.0 { return vec3<f32>(0.0); }
    let scene_local = input.local + cloud_frame.origin_radius.xyz
        - environment.render_origin.xyz;
    let delta = environment.bolid_position_intensity.xyz
        - (input.local + cloud_frame.origin_radius.xyz);
    let light = delta / max(length(delta), 0.000001);
    let view_delta = cloud_frame.eye.xyz - input.local;
    let view = view_delta / max(length(view_delta), 0.000001);
    // light points toward the emitter; incoming photons travel along -light.
    let cosine = clamp(dot(-light, view), -1.0, 1.0);
    let g = 0.65;
    let hg = (1.0-g*g) / (12.5663706144 * pow(1.0+g*g-2.0*g*cosine, 1.5));
    let tau = max(input.parameters.y, 0.0);
    let phase = mix(hg, 0.0795774715, 1.0-exp(-tau));
    // Approximate entry-to-face extinction using the existing optical depth.
    // The unlit boundary transmits less as a cloud thickens; no constant fill.
    let entry_depth = tau * (0.5-0.5*clamp(dot(normal,light),-1.0,1.0));
    return bolid_incident(scene_local) * (0.85 * phase * exp(-entry_depth));
}

fn cloud_color(input: CloudVertex) -> vec4<f32> {
    if hidden(input.local) { discard; }
    let up = normalize(input.local + cloud_frame.origin_radius.xyz);
    let age = input.parameters.x;
    let fade = smoothstep(0.0, 420.0, age) * (1.0 - smoothstep(2800.0, 3600.0, age));
    let alpha = (1.0 - exp(-input.parameters.y)) * fade * edge_fade(up);
    if alpha < 0.0001 { discard; }
    let n = normalize(input.normal);
    let sun = environment.global_sun_direction.xyz;
    let sun_visibility = smoothstep(-0.03, 0.03, dot(up, sun));
    let moon_visibility = smoothstep(-0.03, 0.03, dot(up, environment.moon_apparent_world.xyz));
    var direct = environment.global_sun_rgb.xyz * sun_visibility * (0.24 + 0.76 * max(dot(n, sun), 0.0));
    var lunar = environment.local_moon_rgb_visibility.xyz * moon_visibility
        * (0.3 + 0.7 * max(dot(n, environment.moon_apparent_world.xyz), 0.0));
    var ambient=environment.ambient_rgb_exposure.xyz;
    if physical_atmosphere() {
        direct=physical_sun(input.local)*(0.24+0.76*max(dot(n,sun),0.0));
        lunar=physical_moon(input.local)*(0.3+0.7*max(dot(n,surface_moon_direction()),0.0));
        ambient=physical_ambient(input.local);
    }
    var color = ambient * (0.55 + 0.45 * max(dot(n, up), 0.0)) + direct + lunar
        + cloud_bolid_radiance(input, n);
    if cloud_frame.config.x == 2u {
        let colors = array<vec3<f32>,3>(vec3(0.3,0.8,1.0), vec3(1.0,0.65,0.2), vec3(0.8,0.4,1.0));
        color = colors[u32(input.parameters.z)];
    }
    if cloud_frame.config.x == 4u {
        let colors = array<vec3<f32>,12>(vec3(0.9,0.9,0.9), vec3(1.0,0.6,0.2),
            vec3(1.0,0.2,0.2), vec3(0.3,0.6,0.8), vec3(0.2,0.9,0.5), vec3(0.4,0.4,0.7),
            vec3(0.9,0.8,0.2), vec3(0.8,0.4,0.2), vec3(1.0,0.3,0.8), vec3(0.6,0.9,1.0),
            vec3(0.5,0.3,1.0), vec3(0.8,0.6,1.0));
        color = colors[u32(input.detail.x)];
    }
    if cloud_frame.config.x == 5u {
        color = mix(vec3(0.15,0.3,1.0), vec3(1.0,0.2,0.1), input.detail.y / 6.0);
    }
    if physical_atmosphere() && cloud_frame.config.x == 0u {
        let path=sample_aerial(input.local);
        color=color*path.transmission+path.scattering;
    }
    if cloud_frame.config.x != 0u {return vec4(tone_map_display(color),alpha);}
    return vec4(tone_map_linear(color), alpha);
}

struct WeightedOutput {
    @location(0) accumulation: vec4<f32>,
    @location(1) revealage: f32,
};

@fragment fn fs_oit(input: CloudVertex) -> WeightedOutput {
    let c = cloud_color(input);
    let distance = length(input.local - cloud_frame.eye.xyz) / max(cloud_frame.origin_radius.w * 0.06, 1.0);
    let weight = clamp((0.2 + c.a * 3.0) / (0.2 + distance * distance), 0.05, 12.0);
    return WeightedOutput(vec4(c.rgb * c.a, c.a) * weight, c.a);
}
@fragment fn fs_alpha(input: CloudVertex) -> @location(0) vec4<f32> { return cloud_color(input); }

// Reuse face opacity, but only faces before the emissive layer attenuate it.
struct BolidCloudOutput {
    @location(0) opacity: vec4<f32>,
    @location(1) foreground: vec4<f32>,
};
@fragment fn fs_bolid_mask(input: CloudVertex) -> BolidCloudOutput {
    let stored=textureLoad(bolid_distance,vec2<i32>(input.position.xy),0).rg;
    if stored.y<=0.000001 {discard;}
    let distance=length(input.local-cloud_frame.eye.xyz)/1000000.0;
    if distance>=stored.x/stored.y {discard;}
    let c=cloud_color(input);
    let relative_distance=length(input.local-cloud_frame.eye.xyz)/max(cloud_frame.origin_radius.w*0.06,1.0);
    let weight=clamp((0.2+c.a*3.0)/(0.2+relative_distance*relative_distance),0.05,12.0);
    return BolidCloudOutput(vec4(c.a),vec4(c.rgb*c.a,c.a)*weight);
}

struct DebugVertex {
    @builtin(position) position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
};
@vertex fn vs_debug(@builtin(vertex_index) index: u32) -> DebugVertex {
    let p = array<vec2<f32>,3>(vec2(-1.0,-1.0), vec2(3.0,-1.0), vec2(-1.0,3.0))[index];
    return DebugVertex(vec4(p, 0.0, 1.0), p);
}
@fragment fn fs_debug(input: DebugVertex) -> @location(0) vec4<f32> {
    if cloud_frame.config.x == 2u || cloud_frame.config.x >= 4u { discard; }
    let projected = cloud_frame.inverse_projection * vec4(input.ndc, 0.5, 1.0);
    let ray = normalize(projected.xyz / projected.w - cloud_frame.eye.xyz);
    let eye = cloud_frame.eye.xyz + cloud_frame.origin_radius.xyz;
    let radius = cloud_frame.origin_radius.w * (1.0 + 1800.0 / 6371000.0);
    let b = dot(eye, ray);
    let c = (length(eye) - radius) * (length(eye) + radius);
    let d = b * b - c;
    if d < 0.0 { discard; }
    var hit = -b - sqrt(d);
    if hit < 0.0 { hit = -b + sqrt(d); }
    if hit < 0.0 { discard; }
    let local = cloud_frame.eye.xyz + ray * hit;
    if hidden(local) { discard; }
    let up = normalize(eye + ray * hit);
    let fade = edge_fade(up);
    if fade <= 0.0 { discard; }
    if cloud_frame.config.x == 1u {
        let grid = region_uv(up) * 128.0;
        let width = max(fwidth(grid), vec2(0.008));
        let line = min(fract(grid), 1.0 - fract(grid)) / width;
        let alpha = 1.0 - smoothstep(0.5, 1.5, min(line.x, line.y));
        return vec4(0.1, 0.95, 0.75, alpha * fade * 0.8);
    }
    // East-facing arrows on a spherical shell, including cloud-free regions.
    let lon = atan2(up.z, up.x);
    let lat = asin(up.y);
    let p = fract(vec2(lon * cos(lat), lat) * 700.0) - vec2(0.5);
    let shaft = abs(p.y) < 0.035 && abs(p.x) < 0.3;
    let head = abs(abs(p.y) - (0.3 - p.x)) < 0.045 && p.x > 0.05 && p.x < 0.34;
    if !shaft && !head { discard; }
    return vec4(1.0, 0.8, 0.15, fade * 0.7);
}
