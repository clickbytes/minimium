struct MoonVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) local_position: vec2<f32>,
    @location(1) moon_direction: vec3<f32>,
    @location(2) disk_up: vec3<f32>,
    @location(3) disk_right: vec3<f32>,
};

struct MoonDiskBasis {
    right: vec3<f32>,
    up: vec3<f32>,
};

fn moon_disk_basis(direction: vec3<f32>, pole: vec3<f32>) -> MoonDiskBasis {
    let projected_pole = pole - direction * dot(pole, direction);
    let reference = select(
        vec3<f32>(0.0, 1.0, 0.0),
        vec3<f32>(1.0, 0.0, 0.0),
        abs(direction.y) > 0.9,
    );
    let fallback = reference - direction * dot(reference, direction);
    let up = normalize(select(fallback, projected_pole, dot(projected_pole, projected_pole) > 1.0e-10));
    let right = normalize(cross(direction, up));
    return MoonDiskBasis(right, up);
}

@vertex fn vs_main(@builtin(vertex_index) vertex_index: u32) -> MoonVertexOutput {
    let corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[vertex_index];
    let ray = visual_moon_ray();
    let direction = select(vec3<f32>(0.0,1.0,0.0),ray.xyz,ray.w>0.0);
    let basis = moon_disk_basis(direction, normalize(environment.moon_pole_world.xyz));
    let radius = asin(clamp(ray.w,0.0,0.999999));
    let disk_direction = normalize(
        direction + (basis.right * corner.x + basis.up * corner.y) * tan(radius),
    );
    var clip = camera.sky_view_projection * vec4<f32>(disk_direction, 0.0);
    if clip.w <= 0.00001 || ray.w<=0.0 {
        clip = vec4<f32>(2.0, 2.0, 2.0, 1.0);
    } else {
        clip = vec4<f32>(clip.xy, clip.w, clip.w);
    }
    var output: MoonVertexOutput;
    output.clip_position = clip;
    output.local_position = corner;
    output.moon_direction = direction;
    output.disk_up = basis.up;
    output.disk_right = basis.right;
    return output;
}

@fragment fn fs_main(input: MoonVertexOutput) -> @location(0) vec4<f32> {
    let radius_squared = dot(input.local_position, input.local_position);
    let edge_width = max(fwidth(radius_squared), 0.00001);
    let coverage = 1.0 - smoothstep(1.0 - edge_width, 1.0 + edge_width, radius_squared);
    if coverage <= 0.0 {
        discard;
    }
    let ndc = vec2<f32>(
        input.clip_position.x * camera.viewport.z * 2.0 - 1.0,
        1.0 - input.clip_position.y * camera.viewport.w * 2.0,
    );
    let far_point = camera.inverse_sky_view_projection * vec4<f32>(ndc, 1.0, 1.0);
    let ray_world = normalize(far_point.xyz / max(abs(far_point.w), 0.000001));
    let horizon = horizon_visibility(ray_world);
    if horizon <= 0.0 {
        discard;
    }
    let z = sqrt(max(1.0 - radius_squared, 0.0));
    let surface_normal = normalize(
        input.disk_right * input.local_position.x
            + input.disk_up * input.local_position.y
            - input.moon_direction * z,
    );
    let lunar_radiance = lunar_surface_radiance(surface_normal);
    var linear_color = lunar_radiance;
    if environment.flags.y == 0u && !physical_atmosphere() {
        let atmosphere = sample_clear_sky(ray_world);
        linear_color = atmosphere.scattering_rgb + atmosphere.transmittance * lunar_radiance;
    }
    let alpha = coverage * horizon;
    let output_color = tone_map_linear(linear_color);
    return vec4<f32>(output_color * alpha, alpha);
}
