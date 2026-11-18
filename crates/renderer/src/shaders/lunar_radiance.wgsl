// Apparent V flux already includes the phase. Normalize the shading across
// the projected disk to keep its integral consistent for every terminator.
fn lunar_surface_radiance(normal: vec3<f32>) -> vec3<f32> {
    let incidence = dot(normal, normalize(environment.moon_light_world_phase.xyz));
    if incidence <= 0.0 { return vec3<f32>(0.0); }
    let fraction = clamp(environment.moon_light_world_phase.w, 0.0, 1.0);
    let cosine = 2.0*fraction - 1.0;
    let phase = acos(cosine);
    let lambert_mean = (2.0/3.0) * (sin(phase) + (3.14159265-phase)*cosine) / 3.14159265;
    // The full Moon shows a much flatter lit limb than a Lambertian sphere.
    let opposition = smoothstep(0.90, 0.995, fraction)*0.85;
    let shape = mix(incidence, 1.0, opposition);
    let mean = mix(lambert_mean, fraction, opposition);
    let disk = environment.moon_disk_radiance;
    let fallback_gain=select(max(environment.star_animation.z,1.0),1.0,physical_atmosphere());
    return disk.rgb * disk.w * (shape/max(mean, 0.00001)) * fallback_gain;
}
