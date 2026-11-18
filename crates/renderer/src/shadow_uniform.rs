//! Shader layout and deterministic directional shadow projection preparation.

use crate::ShadowSettings;

const SHADOW_SOURCE_NONE: u32 = 0;

/// Uniform shared by shadow depth and sampling passes at every viewing scale.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
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

/// Snap in a nearby world-anchored light grid. A floating-origin shift does
/// not move the grid, and rotation of the light has only a local lever arm.
pub fn prepare_at_origin(
    direction_to_body: [f64; 3],
    center_world: [f64; 3],
    render_origin: [f64; 3],
    settings: ShadowSettings,
    source_id: u32,
) -> Option<ShadowUniform> {
    DirectionalShadowProjection::new(direction_to_body, center_world, settings)?
        .uniform(render_origin, source_id)
}

/// Directional shadow footprint with an independently fitted depth toward the light.
/// The same snapped square determines both the caster query and the GPU projection.
pub struct DirectionalShadowProjection {
    center: glam::DVec3,
    snapped_center: glam::DVec3,
    to_body: glam::DVec3,
    right: glam::DVec3,
    up: glam::DVec3,
    half_extent: f64,
    sunward_reach: f64,
    settings: ShadowSettings,
}

impl DirectionalShadowProjection {
    pub fn new(
        direction_to_body: [f64; 3],
        center_world: [f64; 3],
        settings: ShadowSettings,
    ) -> Option<Self> {
        let to_body = glam::DVec3::from_array(direction_to_body).try_normalize()?;
        let center = glam::DVec3::from_array(center_world);
        if !center.is_finite()
            || !settings.receiver_radius_m.is_finite()
            || !settings.caster_radius_m.is_finite()
            || settings.receiver_radius_m <= 0.0
            || settings.caster_radius_m < settings.receiver_radius_m
        {
            return None;
        }
        let view_direction = -to_body;
        let reference_up = if view_direction.cross(glam::DVec3::Y).length_squared() > 1e-12 {
            glam::DVec3::Y
        } else {
            glam::DVec3::X
        };
        let right = view_direction.cross(reference_up).normalize();
        let up = right.cross(view_direction).normalize();
        let half_extent = f64::from(settings.receiver_radius_m) * 1.01;
        let grid_width = 2.0 * half_extent;
        let texel_m = grid_width / f64::from(settings.resolution.max(1));
        // Anchor close to the observer so a rotating light has only a local lever arm.
        let anchor = (center / grid_width).round() * grid_width;
        let local = center - anchor;
        let light_x = local.dot(right);
        let light_y = local.dot(up);
        let snapped_x = (light_x / texel_m).round() * texel_m;
        let snapped_y = (light_y / texel_m).round() * texel_m;
        let snapped_center = center + right * (snapped_x - light_x) + up * (snapped_y - light_y);
        Some(Self {
            center,
            snapped_center,
            to_body,
            right,
            up,
            half_extent,
            sunward_reach: f64::from(settings.caster_radius_m) * 1.01,
            settings,
        })
    }

    /// Select a resident sector whose bounds can project into this map. The map
    /// width determines contact resolution even for a ridge kilometres upstream.
    pub fn include_caster(&mut self, bounds: crate::shadow_geometry::ShadowBounds) -> bool {
        let lo = glam::DVec3::from_array(bounds.minimum) - self.snapped_center;
        let hi = glam::DVec3::from_array(bounds.maximum) - self.snapped_center;
        let midpoint = (lo + hi) * 0.5;
        let half = (hi - lo) * 0.5;
        for axis in [self.right, self.up] {
            if midpoint.dot(axis).abs() > self.half_extent + half.dot(axis.abs()) {
                return false;
            }
        }
        // Intersect the center ray with the AABB expanded by the footprint.
        // This also bounds the depth of very coarse diagonal planetary patches
        // without fitting the shadow map to their most remote, off-map corner.
        let padding = (self.right.abs() + self.up.abs()) * self.half_extent;
        let mut start = -f64::from(self.settings.receiver_radius_m);
        let mut end = midpoint.dot(self.to_body) + half.dot(self.to_body.abs());
        for axis in 0..3 {
            let low = lo[axis] - padding[axis];
            let high = hi[axis] + padding[axis];
            let direction = self.to_body[axis];
            if direction == 0.0 {
                if low > 0.0 || high < 0.0 {
                    return false;
                }
            } else {
                let a = low / direction;
                let b = high / direction;
                start = start.max(a.min(b));
                end = end.min(a.max(b));
            }
        }
        if end < start {
            return false;
        }
        self.sunward_reach = self.sunward_reach.max(end);
        true
    }

    pub fn uniform(self, render_origin: [f64; 3], source_id: u32) -> Option<ShadowUniform> {
        let origin = glam::DVec3::from_array(render_origin);
        if !origin.is_finite() {
            return None;
        }
        let back = f64::from(self.settings.caster_radius_m) * 1.01;
        // Quantize only an extended reach; small changes to faraway bounds
        // should not rescale the depth map at every solar update.
        let front = if self.sunward_reach > back {
            let step = (self.sunward_reach.log2().floor() - 4.0).exp2();
            ((self.sunward_reach + 1.0) / step).ceil() * step
        } else {
            back
        };
        let relative = self.snapped_center - origin;
        let eye = relative + self.to_body * (front + 1.0);
        let view = glam::DMat4::look_to_rh(eye, -self.to_body, self.up);
        let near = 1.0;
        let far = 1.0 + front + back;
        let extent = self.half_extent;
        let projection = glam::DMat4::orthographic_rh(-extent, extent, -extent, extent, near, far);
        let matrix = (projection * view).as_mat4();
        let settings = self.settings;
        matrix.is_finite().then(|| ShadowUniform {
            light_view_projection: matrix.to_cols_array_2d(),
            receiver_center_relative_radius: (self.center - origin)
                .as_vec3()
                .extend(settings.receiver_radius_m)
                .to_array(),
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
        })
    }
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
