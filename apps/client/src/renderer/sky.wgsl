struct SkyVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
};

@group(1) @binding(0) var milky_way_map: texture_2d<f32>;
@group(1) @binding(1) var milky_way_sampler: sampler;

fn milky_way_radiance(ray_world: vec3<f32>) -> vec3<f32> {
    let eqj = normalize((transpose(environment.eqj_to_world) * vec4<f32>(ray_world, 0.0)).xyz);
    // NASA SVS celestial plate carree: RA 0 at the centre, RA increases left.
    let u = fract(0.5 - atan2(eqj.y, eqj.x) / 6.2831853);
    let v = 0.5 - asin(clamp(eqj.z, -1.0, 1.0)) / 3.14159265;
    let source = textureSample(milky_way_map, milky_way_sampler, vec2<f32>(u, v)).rgb;
    let luminance = dot(source, vec3<f32>(0.2126, 0.7152, 0.0722));
    // Dark-adapted vision sees little colour in diffuse galactic light.
    return mix(vec3<f32>(luminance), source, 0.35);
}

@vertex fn vs_main(@builtin(vertex_index) index: u32) -> SkyVertexOutput {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    var output: SkyVertexOutput;
    output.clip_position = vec4<f32>(positions[index], 1.0, 1.0);
    return output;
}

@fragment fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let ndc = vec2<f32>(
        position.x * camera.viewport.z * 2.0 - 1.0,
        1.0 - position.y * camera.viewport.w * 2.0,
    );
    let far_point = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray_world = normalize(far_point.xyz / max(abs(far_point.w), 1.0e-6));
    if environment.flags.y == 1u {
        return vec4<f32>(tone_map_linear(milky_way_radiance(ray_world) * 0.03), 1.0);
    }
    let sample = sample_clear_sky(ray_world);
    let moon_flux = max(environment.moon_disk_radiance.w, 0.0)
        * environment.local_moon_rgb_visibility.w;
    let moon_contrast = 1.0 / (1.0 + 8.0 * moon_flux);
    let galactic_visibility = sample.transmittance
        * environment.sky_horizon_rgb_star_visibility.w * moon_contrast;
    var galactic_light = vec3<f32>(0.0);
    if environment.sky_horizon_rgb_star_visibility.w > 0.0 {
        galactic_light = milky_way_radiance(ray_world);
    }
    let sky = tone_map_linear(sample.scattering_rgb
        + galactic_light * 0.02 * galactic_visibility);
    let ground = HORIZON_OCCLUSION_RGB;
    let visibility = horizon_visibility(ray_world);
    return vec4<f32>(mix(ground, sky, visibility), 1.0);
}
