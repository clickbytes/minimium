//! Shader layout and deterministic directional shadow projection preparation.

use crate::ShadowSettings;

const SHADOW_SOURCE_NONE: u32 = 0;

/// Uniform shared by shadow depth and sampling passes at every viewing scale.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShadowUniform {
    pub light_view_projection: [[f32; 4]; 4],
    pub receiver_center_relative_radius: [f32; 4],
    pub texel_depth_range: [f32; 4],
    pub bias_fade: [f32; 4],
    pub flags: [u32; 4],
}

/// Builds a stable light-space projection for one directional shadow source.
pub fn prepare(
    direction_to_body: [f64; 3],
    center_relative: [f64; 3],
    settings: ShadowSettings,
    source_id: u32,
) -> Option<ShadowUniform> {
    prepare_at_origin(
        direction_to_body,
        center_relative,
        [0.0; 3],
        settings,
        source_id,
    )
}

/// Snap in absolute f64 light space so a floating-origin shift does not move
/// the shadow texels. Cast to f32 only after constructing the relative matrix.
pub fn prepare_at_origin(
    direction_to_body: [f64; 3],
    center_world: [f64; 3],
    render_origin: [f64; 3],
    settings: ShadowSettings,
    source_id: u32,
) -> Option<ShadowUniform> {
    let to_body = glam::DVec3::from_array(direction_to_body).normalize();
    let center = glam::DVec3::from_array(center_world);
    let origin = glam::DVec3::from_array(render_origin);
    if !to_body.is_finite()
        || !center.is_finite()
        || !origin.is_finite()
        || to_body.length_squared() < 0.99
        || settings.receiver_radius_m <= 0.0
        || settings.caster_radius_m < settings.receiver_radius_m
    {
        return None;
    }
    let view_direction = -to_body;
    let reference_up = least_parallel_axis(view_direction);
    let right = view_direction.cross(reference_up).normalize();
    let up = right.cross(view_direction).normalize();
    let half_extent = f64::from(settings.receiver_radius_m) * 1.01;
    let texel_m = half_extent * 2.0 / f64::from(settings.resolution.max(1));
    let light_x = center.dot(right);
    let light_y = center.dot(up);
    let snapped_x = (light_x / texel_m).round() * texel_m;
    let snapped_y = (light_y / texel_m).round() * texel_m;
    let snapped_center = center + right * (snapped_x - light_x) + up * (snapped_y - light_y);
    let depth_radius = f64::from(settings.caster_radius_m) * 1.01;
    let relative = snapped_center - origin;
    let eye = relative + to_body * (depth_radius + 1.0);
    let view = glam::DMat4::look_at_rh(eye, relative, up);
    let near = 1.0;
    let far = 1.0 + 2.0 * depth_radius;
    let projection = glam::DMat4::orthographic_rh(
        -half_extent,
        half_extent,
        -half_extent,
        half_extent,
        near,
        far,
    );
    let matrix = (projection * view).as_mat4();
    let center_relative = (center - origin).to_array();
    let uniform = ShadowUniform {
        light_view_projection: matrix.to_cols_array_2d(),
        receiver_center_relative_radius: [
            center_relative[0] as f32,
            center_relative[1] as f32,
            center_relative[2] as f32,
            settings.receiver_radius_m,
        ],
        texel_depth_range: [
            1.0 / settings.resolution.max(1) as f32,
            1.0 / settings.resolution.max(1) as f32,
            near as f32,
            far as f32,
        ],
        bias_fade: [
            settings.constant_bias_m,
            settings.slope_bias_m,
            settings.max_bias_m,
            settings.fade_start_m,
        ],
        flags: [1, source_id, 0, 0],
    };
    uniform
        .light_view_projection
        .iter()
        .flatten()
        .all(|value| value.is_finite())
        .then_some(uniform)
}

fn least_parallel_axis(direction: glam::DVec3) -> glam::DVec3 {
    let axes = [glam::DVec3::X, glam::DVec3::Y, glam::DVec3::Z];
    *axes
        .iter()
        .min_by(|left, right| {
            direction
                .dot(**left)
                .abs()
                .total_cmp(&direction.dot(**right).abs())
        })
        .expect("basis axes are nonempty")
}

/// Perspective shadow cone covering the bounded receiver region. The emitter
/// remains omnidirectional; only shadow-map coverage is restricted to this cone.
pub fn prepare_point_at_origin(
    source: [f64; 3],
    center: [f64; 3],
    origin: [f64; 3],
    settings: ShadowSettings,
) -> Option<ShadowUniform> {
    let source = glam::DVec3::from_array(source);
    let center = glam::DVec3::from_array(center);
    let origin = glam::DVec3::from_array(origin);
    let delta = center - source;
    let distance = delta.length();
    let radius = f64::from(settings.caster_radius_m) * 1.05;
    if !distance.is_finite() || distance <= radius {
        return None;
    }
    let near = (distance - radius).max(1.0);
    let far = distance + radius;
    // Only rays through receivers can contribute visible shadows. Keep the
    // larger caster radius for depth and selection, not angular coverage:
    // using it for FOV wastes most texels outside the receiving region.
    let receiver_radius = f64::from(settings.receiver_radius_m) * 1.01;
    if receiver_radius <= 0.0 || receiver_radius > radius {
        return None;
    }
    let fov = 2.0 * (receiver_radius / distance).asin();
    let view = glam::DMat4::look_at_rh(
        source - center,
        glam::DVec3::ZERO,
        least_parallel_axis(delta.normalize()),
    );
    // Point-shadow shaders subtract the receiver center before projection.
    // Keep f32 dot products local to this bounded region, even when the scene
    // origin lies far away or the source is tens of kilometres above it.
    let matrix = glam::DMat4::perspective_rh(fov, 1.0, near, far) * view;
    Some(ShadowUniform {
        light_view_projection: matrix.as_mat4().to_cols_array_2d(),
        receiver_center_relative_radius: (center - origin)
            .as_vec3()
            .extend(settings.receiver_radius_m)
            .to_array(),
        texel_depth_range: [
            1.0 / settings.resolution as f32,
            1.0 / settings.resolution as f32,
            near as f32,
            far as f32,
        ],
        bias_fade: [
            settings.constant_bias_m,
            settings.slope_bias_m,
            settings.max_bias_m,
            settings.fade_start_m,
        ],
        // Point-map occlusion weight; the client can fade its bounded takeover.
        flags: [1, 3, 1.0_f32.to_bits(), 0],
    })
}

const _: () = {
    assert!(std::mem::size_of::<ShadowUniform>() == 128);
    assert!(std::mem::align_of::<ShadowUniform>() == 16);
    assert!(std::mem::offset_of!(ShadowUniform, light_view_projection) == 0);
    assert!(std::mem::offset_of!(ShadowUniform, receiver_center_relative_radius) == 64);
    assert!(std::mem::offset_of!(ShadowUniform, texel_depth_range) == 80);
    assert!(std::mem::offset_of!(ShadowUniform, bias_fade) == 96);
    assert!(std::mem::offset_of!(ShadowUniform, flags) == 112);
    assert!(SHADOW_SOURCE_NONE == 0);
};
