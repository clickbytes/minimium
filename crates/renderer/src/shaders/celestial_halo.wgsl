struct HaloVertex {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) @interpolate(flat) body: u32,
};

fn halo_source(body: u32) -> HaloSource {
    return environment.halo_sources[body];
}

fn halo_direction(body: u32) -> vec3<f32> {
    let source = halo_source(body);
    if u32(source.parameters.x) == 0u && has_spherical_atmosphere() { return visual_sun_direction(); }
    if u32(source.parameters.x) == 1u && environment.flags.y == 0u && has_spherical_atmosphere() {
        return visual_moon_ray().xyz;
    }
    return normalize(source.direction_extent.xyz);
}

fn finite_scalar(value: f32) -> bool {
    return abs(value) <= 3.402823e38;
}

fn finite_vec3(value: vec3<f32>) -> bool {
    return all(abs(value) <= vec3<f32>(3.402823e38));
}

@vertex fn vs_main(@builtin(vertex_index) index: u32, @builtin(instance_index) body: u32) -> HaloVertex {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let source = halo_source(body);
    let center = halo_direction(body);
    let extent = max(source.direction_extent.w, 0.00001);
    let reference = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(center.y) > 0.95);
    let right = normalize(cross(reference, center));
    let up = normalize(cross(center, right));
    let ray = normalize(center + (right * corners[index].x + up * corners[index].y) * tan(extent));
    let clip = camera.sky_view_projection * vec4<f32>(ray, 0.0);
    var output: HaloVertex;
    output.body = body;
    if clip.w <= 0.00001 || body >= environment.halo_count.x {
        output.clip_position = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    } else {
        output.clip_position = vec4<f32>(clip.xy, clip.w, clip.w);
    }
    return output;
}

@fragment fn fs_main(input: HaloVertex) -> @location(0) vec4<f32> {
    if input.body >= min(environment.halo_count.x, 8u) {
        discard;
    }
    let source = halo_source(input.body);
    let extent = source.direction_extent.w;
    var visibility = source.linear_rgb_visibility.w;
    if !finite_scalar(extent) || !finite_scalar(visibility)
        || !finite_vec3(source.direction_extent.xyz)
        || !finite_vec3(source.linear_rgb_visibility.xyz)
        || !finite_vec3(source.source_direction.xyz)
        || extent <= 0.0 || visibility <= 0.0 {
        discard;
    }
    let ndc = vec2<f32>(
        input.clip_position.x * camera.viewport.z * 2.0 - 1.0,
        1.0 - input.clip_position.y * camera.viewport.w * 2.0,
    );
    let far_point = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray = normalize(far_point.xyz / max(abs(far_point.w), 0.000001));
    let center = halo_direction(input.body);
    let angle = atan2(length(cross(ray, center)), dot(ray, center));
    let horizon = horizon_visibility(ray);
    if horizon <= 0.0 {
        discard;
    }
    let shape = u32(source.parameters.x);
    if shape<=1u && has_spherical_atmosphere() {
        let limb=camera_horizon();
        let radius=select(sin(environment.sun_apparent_world.w),visual_moon_ray().w,shape==1u);
        let half_width=max(radius*sqrt(max(1.0-limb.w*limb.w,0.0)),1e-7);
        visibility=smoothstep(limb.w-half_width,limb.w+half_width,dot(limb.xyz,center));
        if visibility<=0.0 {discard;}
    }
    var envelope = exp(-angle / max(extent * 0.25, 0.0001))
        * (1.0 - smoothstep(extent * 0.875, extent, angle));
    var radiance = source.linear_rgb_visibility.xyz * envelope;
    if shape == 0u {
        envelope = exp(-angle / radians(1.7))
            * (1.0 - smoothstep(extent * 0.875, extent, angle));
        radiance = source.linear_rgb_visibility.xyz * envelope;
    } else if shape == 1u {
        if physical_atmosphere() {
            // A compact droplet corona rides on the broad Mie glow already
            // integrated in the atmosphere. Its energy follows the Moon's flux.
            if atmosphere.fog_time.z <= 0.0 || angle >= extent { discard; }
            let tau = sample_sky(input.clip_position.xy * camera.viewport.zw).fog;
            let moisture = (1.0 - exp(-tau)) * (1.0 - smoothstep(0.8, 3.0, tau));
            if moisture <= 0.0 { discard; }
            let width = extent * 0.20;
            let a = angle / width;
            let ring_offset = (a - 2.3)/0.30;
            let ring = exp(-0.5 * ring_offset * ring_offset);
            let profile = (exp(-a) + 0.12*ring) * (1.0-smoothstep(0.85*extent, extent, angle));
            let radius = environment.moon_orbit_world.w / max(length(environment.moon_orbit_world.xyz-camera.eye_world.xyz), 1.0);
            let angular_gain = 0.5 * pow(min(radius/width, 1.0), 2.0);
            let lunar = environment.moon_disk_radiance;
            radiance = lunar.rgb * lunar.w * source.linear_rgb_visibility.rgb * angular_gain * moisture * profile;
            return vec4<f32>(tone_map_linear(radiance * visibility * horizon), 0.0);
        }
        let phase = clamp(source.parameters.y, 0.0, 1.0);
        let sun = normalize(source.source_direction.xyz);
        let tangent = sun - center * dot(sun, center);
        let limb = ray - center * dot(ray, center);
        let facing = dot(normalize(tangent + vec3<f32>(0.000001)), normalize(limb + vec3<f32>(0.000001)));
        let orientation = 1.0 + 0.45 * (1.0 - phase) * facing;
        envelope = exp(-angle / radians(0.9))
            * (1.0 - smoothstep(extent * 0.875, extent, angle));
        radiance = source.linear_rgb_visibility.xyz * envelope * pow(phase, 1.4) * orientation;
    } else if shape == 3u {
        let source_direction = normalize(source.source_direction.xyz);
        let phase = clamp(source.parameters.y, 0.0, 1.0);
        let facing = max(dot(center, source_direction), 0.0);
        radiance *= phase * mix(0.35, 1.0, facing);
    } else if shape == 4u {
        radiance *= 1.25;
    }
    if physical_atmosphere() && shape == 0u { discard; }
    return vec4<f32>(tone_map_display(radiance * visibility * horizon), 0.0);
}
