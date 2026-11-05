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
    @location(3) @interpolate(flat) catalog_star_id: u32,
};

const STAR_REFERENCE_MAGNITUDE = 1.0;
const STAR_RADIANCE_SCALE = 3.0;

fn star_prominence(visual_magnitude: f32) -> f32 {
    return clamp((5.0 - visual_magnitude) / 6.5, 0.0, 1.0);
}

fn star_core_radius_pixels(visual_magnitude: f32) -> f32 {
    return mix(1.4, 6.5, pow(star_prominence(visual_magnitude), 1.1));
}

fn star_radius_pixels(visual_magnitude: f32) -> f32 {
    return star_core_radius_pixels(visual_magnitude)
        + mix(1.5, 3.5, star_prominence(visual_magnitude));
}

fn star_hash(id: u32) -> f32 {
    var value = id ^ 0x9e3779b9u;
    value = (value ^ (value >> 16u)) * 0x7feb352du;
    value = (value ^ (value >> 15u)) * 0x846ca68bu;
    return f32(value ^ (value >> 16u)) / 4294967295.0;
}

@vertex fn vs_main(input: StarInput, @builtin(vertex_index) vertex_index: u32) -> StarVertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[vertex_index];
    let direction_world = normalize((environment.eqj_to_world * vec4<f32>(input.direction_eqj, 0.0)).xyz);
    var clip = camera.sky_view_projection * vec4<f32>(direction_world, 0.0);
    let radius_pixels = star_radius_pixels(input.visual_magnitude);
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
    output.catalog_star_id = input.catalog_star_id;
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
    let horizon = horizon_visibility(ray_world);
    let overview = environment.flags.y == 1u;
    var background = vec3<f32>(0.0);
    var transmission = 1.0;
    if !overview {
        let atmosphere = sample_clear_sky(ray_world);
        background = atmosphere.scattering_rgb;
        transmission = atmosphere.transmittance * environment.sky_horizon_rgb_star_visibility.w;
    }
    let visibility = horizon * transmission;
    if visibility <= 0.0 {
        discard;
    }
    let prominence = star_prominence(input.visual_magnitude);
    let core_radius_pixels = star_core_radius_pixels(input.visual_magnitude);
    let radius_pixels = star_radius_pixels(input.visual_magnitude);
    let inner_position = input.local_position * (radius_pixels / core_radius_pixels);
    let inner_radius_squared = dot(inner_position, inner_position);
    let core = exp(-9.0 * inner_radius_squared);
    let glow = prominence * 0.34 * exp(-3.5 * inner_radius_squared);
    let glint = pow(prominence, 4.0) * 0.28
        * exp(-42.0 * min(abs(inner_position.x), abs(inner_position.y)))
        * exp(-3.0 * inner_radius_squared);
    let inner_profile = (core + glow + glint)
        * (1.0 - smoothstep(0.72, 1.0, inner_radius_squared));
    let micro_halo = 0.055 * prominence * prominence * exp(-3.0 * radius_squared)
        * (1.0 - smoothstep(0.75, 1.0, radius_squared));
    let profile = inner_profile + micro_halo;
    let area_compensation = inverseSqrt(core_radius_pixels * 0.5);
    let relative_flux = pow(10.0, -0.4 * (input.visual_magnitude - STAR_REFERENCE_MAGNITUDE));
    let seed = star_hash(input.catalog_star_id);
    let second_seed = star_hash(input.catalog_star_id ^ 0x68bc21ebu);
    let altitude = clamp(dot(normalize(environment.observer_up_horizon_cos.xyz), ray_world), 0.0, 1.0);
    let twinkle_strength = mix(0.28, 0.08, altitude);
    let time = environment.star_animation.x;
    let twinkle = 1.0 + twinkle_strength * (
        0.7 * sin(time * (5.0 + 5.0 * seed) + 6.2831853 * second_seed)
        + 0.3 * sin(time * (10.0 + 7.0 * second_seed) + 6.2831853 * seed)
    );
    let base = tone_map_linear(background);
    let star_radiance = input.color_linear * relative_flux * STAR_RADIANCE_SCALE
        * area_compensation * profile * visibility * twinkle;
    if environment.flags.w != 0u {
        return vec4<f32>(star_radiance, 0.0);
    }
    let with_star = tone_map_linear(background + star_radiance);
    let linear_delta = max(with_star - base, vec3<f32>(0.0));
    return vec4<f32>(linear_delta, 0.0);
}
