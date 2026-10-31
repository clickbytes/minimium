// CPU counterpart: environment/weather/clouds/density.rs. Units are render metres.
struct CloudInstanceGpu {
    center_a: vec4<f32>,
    axes_a: vec4<f32>,
    orientation_a: vec4<f32>,
    base_a: vec4<f32>,
    center_b: vec4<f32>,
    axes_b: vec4<f32>,
    orientation_b: vec4<f32>,
    base_b: vec4<f32>,
    neighbors_a: vec4<u32>,
    neighbors_b: vec4<u32>,
    material: vec4<f32>,
};
struct InstanceUniform {
    eye: vec4<f32>,
    origin: vec4<f32>,
    local_region: vec4<f32>,
    config: vec4<f32>,
    viewport: vec4<f32>,
};
@group(3) @binding(0) var<storage, read> cloud_instances: array<CloudInstanceGpu>;
@group(3) @binding(1) var<uniform> cloud_frame: InstanceUniform;

struct Lobe {
    center: vec3<f32>,
    axes: vec3<f32>,
    rotation: vec4<f32>,
    base: vec4<f32>,
    density: f32,
};
fn rotate(q: vec4<f32>, p: vec3<f32>) -> vec3<f32> {
    return p + 2.0 * cross(q.xyz, cross(q.xyz, p) + q.w * p);
}
fn inverse_rotate(q: vec4<f32>, p: vec3<f32>) -> vec3<f32> {
    return rotate(vec4<f32>(-q.xyz, q.w), p);
}
fn slerp_short(a: vec4<f32>, raw_b: vec4<f32>, t: f32) -> vec4<f32> {
    let b = select(raw_b, -raw_b, dot(a, raw_b) < 0.0);
    let cosine = clamp(dot(a, b), 0.0, 1.0);
    if cosine > 0.9995 { return normalize(mix(a, b, t)); }
    let angle = acos(cosine);
    return normalize((a * sin((1.0 - t) * angle) + b * sin(t * angle)) / sin(angle));
}
fn cap_primitive(z: f32) -> f32 {
    let z2 = z * z;
    return z * (1.0 - z2 + 0.6 * z2 * z2 - z2 * z2 * z2 / 7.0);
}
fn lobe_state(instance: CloudInstanceGpu) -> Lobe {
    let t = weather.event.y;
    var lobe: Lobe;
    lobe.center = mix(instance.center_a.xyz, instance.center_b.xyz, t);
    lobe.axes = exp(mix(log(instance.axes_a.xyz), log(instance.axes_b.xyz), t));
    lobe.rotation = slerp_short(instance.orientation_a, instance.orientation_b, t);
    let plane = mix(instance.base_a, instance.base_b, t);
    lobe.base = plane / length(plane.xyz);
    let n = inverse_rotate(lobe.rotation, lobe.base.xyz) * lobe.axes;
    let z = clamp((lobe.base.w - dot(lobe.center, lobe.base.xyz)) / length(n), -1.0, 1.0);
    let volume = 3.14159265359 / 3.0 * (cap_primitive(1.0) - cap_primitive(z))
        * lobe.axes.x * lobe.axes.y * lobe.axes.z;
    lobe.density = mix(instance.axes_a.w, instance.axes_b.w, t) / max(volume, 1e-15);
    return lobe;
}
// entry, exit, closest approach, squared half-width of the normalized chord.
fn lobe_interval(lobe: Lobe, origin: vec3<f32>, ray: vec3<f32>) -> vec4<f32> {
    let q = inverse_rotate(lobe.rotation, origin - lobe.center) / lobe.axes;
    let d = inverse_rotate(lobe.rotation, ray) / lobe.axes;
    let a = dot(d, d);
    let t = -dot(q, d) / a;
    let closest = q + d * t;
    let h = 1.0 - dot(closest, closest);
    if h <= 0.0 { return vec4<f32>(0.0); }
    let half_width = sqrt(h / a);
    var lo = max(0.0, t - half_width);
    var hi = t + half_width;
    let slope = dot(ray, lobe.base.xyz);
    let offset = lobe.base.w - dot(origin, lobe.base.xyz);
    if abs(slope) < 1e-7 {
        if offset > 0.0 { return vec4<f32>(0.0); }
    } else if slope > 0.0 { lo = max(lo, offset / slope); }
    else { hi = min(hi, offset / slope); }
    return vec4<f32>(lo, hi, t, h);
}
fn kernel_primitive(u: f32, h: f32, a: f32) -> f32 {
    let u2 = u * u;
    return u * (h * h - 2.0 * h * a * u2 / 3.0 + a * a * u2 * u2 / 5.0);
}
fn lobe_integral(lobe: Lobe, ray: vec3<f32>, interval: vec4<f32>, start: f32, end: f32) -> f32 {
    let lo = max(start, interval.x);
    let hi = min(end, interval.y);
    if hi <= lo { return 0.0; }
    let d = inverse_rotate(lobe.rotation, ray) / lobe.axes;
    return max(lobe.density * (kernel_primitive(hi - interval.z, interval.w, dot(d, d))
        - kernel_primitive(lo - interval.z, interval.w, dot(d, d))), 0.0);
}
