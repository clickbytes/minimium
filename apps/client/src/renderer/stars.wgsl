struct StarInput {
    @location(0) direction_eqj: vec3<f32>,
    @location(1) visual_magnitude: f32,
    @location(2) color_linear: vec3<f32>,
    @location(3) catalog_star_id: u32,
};

struct StarVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_position: vec2<f32>,
    @location(1) visual_magnitude: f32,
    @location(2) color_linear: vec3<f32>,
};

const STAR_REFERENCE_MAGNITUDE = 1.0;
const STAR_RADIANCE_SCALE = 0.16;

@vertex fn vs_main(input: StarInput, @builtin(vertex_index) vertex_index: u32) -> StarVertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[vertex_index];
    let direction_world = normalize((environment.eqj_to_world * vec4<f32>(input.direction_eqj, 0.0)).xyz);
    var clip = camera.sky_view_projection * vec4<f32>(direction_world, 0.0);
    let prominence = clamp((2.0 - input.visual_magnitude) / 7.0, 0.0, 1.0);
    let radius_pixels = mix(0.75, 2.5, prominence);
    if clip.w <= 0.00001 {
        clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    } else {
        let pixel_to_ndc = 2.0 * camera.viewport.zw;
        clip = vec4<f32>(
            clip.xy + corner * radius_pixels * pixel_to_ndc * clip.w,
            clip.w,
            clip.w,
        );
    }
    var output: StarVertexOutput;
    output.clip_position = clip;
    output.local_position = corner;
    output.visual_magnitude = input.visual_magnitude;
    output.color_linear = input.color_linear;
    return output;
}

@fragment fn fs_main(input: StarVertexOutput) -> @location(0) vec4<f32> {
    let radius_squared = dot(input.local_position, input.local_position);
    if radius_squared >= 1.0 {
        discard;
    }
    let ndc = vec2<f32>(
        input.clip_position.x * camera.viewport.z * 2.0 - 1.0,
        1.0 - input.clip_position.y * camera.viewport.w * 2.0,
    );
    let far_point = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray_world = normalize(far_point.xyz / max(abs(far_point.w), 0.000001));
    let atmosphere = sample_clear_sky(ray_world);
    let horizon = horizon_visibility(ray_world);
    let sky_star_visibility = environment.sky_horizon_rgb_star_visibility.w;
    let visibility = horizon * atmosphere.transmittance * sky_star_visibility;
    if visibility <= 0.0 {
        discard;
    }
    let profile = exp(-4.0 * radius_squared) * (1.0 - smoothstep(0.72, 1.0, radius_squared));
    let prominence = clamp((2.0 - input.visual_magnitude) / 7.0, 0.0, 1.0);
    let radius_pixels = mix(0.75, 2.5, prominence);
    let inverse_area = 1.0 / (radius_pixels * radius_pixels);
    let relative_flux = pow(10.0, -0.4 * (input.visual_magnitude - STAR_REFERENCE_MAGNITUDE));
    let exposure = environment.ambient_rgb_exposure.w;
    let base = tone_map_linear(atmosphere.scattering_rgb * exposure);
    let star_radiance = input.color_linear * relative_flux * STAR_RADIANCE_SCALE
        * inverse_area * profile * visibility;
    let with_star = tone_map_linear((atmosphere.scattering_rgb + star_radiance) * exposure);
    let linear_delta = max(with_star - base, vec3<f32>(0.0));
    return vec4<f32>(linear_delta, 0.0);
}
