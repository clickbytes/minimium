use super::environment::{EnvironmentFrame, EnvironmentFrameStatus, LocalBodyFrame};
use minimium_procgen::SectorGeometry;

const SHADOW_SOURCE_NONE: u32 = 0;
const SHADOW_SOURCE_SUN: u32 = 1;
const SHADOW_SOURCE_MOON: u32 = 2;

#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct ShadowUniform {
    pub(super) light_view_projection: [[f32; 4]; 4],
    pub(super) receiver_center_relative_radius: [f32; 4],
    pub(super) texel_depth_range: [f32; 4],
    pub(super) bias_fade: [f32; 4],
    pub(super) flags: [u32; 4],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShadowSource {
    Sun,
    Moon,
}

impl ShadowSource {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Sun => "sun",
            Self::Moon => "moon",
        }
    }

    fn id(self) -> u32 {
        match self {
            Self::Sun => SHADOW_SOURCE_SUN,
            Self::Moon => SHADOW_SOURCE_MOON,
        }
    }

    pub(super) fn direction(self, frame: &EnvironmentFrame) -> Option<[f64; 3]> {
        let direction = match self {
            Self::Sun => frame.local_sun?.geometric_direction_world,
            Self::Moon => frame.local_moon?.geometric_direction_world,
        };
        let length = direction
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        (length.is_finite() && length > 1.0e-12).then(|| direction.map(|value| value / length))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ShadowInvalidReason {
    Disabled,
    NoSource,
    InvalidFrame,
    IncompleteCasters,
    BudgetExceeded,
    StaleContext,
}

impl ShadowInvalidReason {
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NoSource => "no_source",
            Self::InvalidFrame => "invalid_frame",
            Self::IncompleteCasters => "incomplete_casters",
            Self::BudgetExceeded => "budget_exceeded",
            Self::StaleContext => "stale_context",
        }
    }
}

#[derive(Default)]
pub(super) struct ShadowState {
    pub(super) previous_source: Option<ShadowSource>,
    pub(super) source: Option<ShadowSource>,
    pub(super) valid: bool,
    pub(super) invalid_reason: Option<ShadowInvalidReason>,
    pub(super) caster_keys: Vec<(minimium_core::world::SectorKey, u8)>,
}

impl ShadowState {
    pub(super) fn invalidate(&mut self, reason: ShadowInvalidReason) {
        self.valid = false;
        self.source = None;
        self.caster_keys.clear();
        self.invalid_reason = Some(reason);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ShadowSettings {
    pub(super) enabled: bool,
    pub(super) resolution: u32,
    pub(super) receiver_radius_m: f32,
    pub(super) caster_radius_m: f32,
    pub(super) fade_start_m: f32,
    pub(super) constant_bias_m: f32,
    pub(super) slope_bias_m: f32,
    pub(super) max_bias_m: f32,
}

impl ShadowSettings {
    pub(super) fn for_quality(quality: crate::environment::quality::SkyQuality) -> Self {
        let profile = quality.profile();
        Self {
            enabled: profile.shadows_enabled,
            resolution: profile.shadow_resolution,
            receiver_radius_m: profile.shadow_receiver_radius_m,
            caster_radius_m: profile.shadow_caster_radius_m,
            fade_start_m: profile.shadow_fade_start_m,
            constant_bias_m: profile.shadow_constant_bias_m,
            slope_bias_m: profile.shadow_slope_bias_m,
            max_bias_m: profile.shadow_max_bias_m,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ShadowBounds {
    pub(super) minimum: [f64; 3],
    pub(super) maximum: [f64; 3],
}

impl ShadowBounds {
    pub(super) fn intersects_sphere(self, center: [f64; 3], radius: f64) -> bool {
        let distance_squared = (0..3)
            .map(|axis| {
                let outside = if center[axis] < self.minimum[axis] {
                    self.minimum[axis] - center[axis]
                } else if center[axis] > self.maximum[axis] {
                    center[axis] - self.maximum[axis]
                } else {
                    0.0
                };
                outside * outside
            })
            .sum::<f64>();
        distance_squared <= radius * radius
    }
}

pub(super) fn select_source(
    frame: &EnvironmentFrame,
    previous: Option<ShadowSource>,
) -> Option<ShadowSource> {
    if frame.status != EnvironmentFrameStatus::Available {
        return None;
    }
    let sun = body_strength(frame.local_sun);
    let moon = body_strength(frame.local_moon);
    let strongest = if sun > 0.0 || moon > 0.0 {
        Some(if sun >= moon {
            ShadowSource::Sun
        } else {
            ShadowSource::Moon
        })
    } else {
        None
    }?;
    let strongest_value = match strongest {
        ShadowSource::Sun => sun,
        ShadowSource::Moon => moon,
    };
    previous
        .filter(|source| {
            let value = match source {
                ShadowSource::Sun => sun,
                ShadowSource::Moon => moon,
            };
            value > 0.0 && value >= strongest_value * 0.95
        })
        .or(Some(strongest))
}

fn body_strength(body: Option<LocalBodyFrame>) -> f32 {
    let Some(body) = body else {
        return 0.0;
    };
    let [red, green, blue] = body.effective_linear_rgb;
    (0.2126 * red + 0.7152 * green + 0.0722 * blue).max(0.0)
}

pub(super) fn shadow_uniform(
    direction_to_body: [f64; 3],
    center_relative: [f64; 3],
    settings: ShadowSettings,
    source: ShadowSource,
) -> Option<ShadowUniform> {
    let to_body = glam::DVec3::from_array(direction_to_body).normalize();
    let center = glam::DVec3::from_array(center_relative);
    if !to_body.is_finite() || !center.is_finite() || to_body.length_squared() < 0.99 {
        return None;
    }
    let view_direction = -to_body;
    let reference_up = least_parallel_axis(view_direction);
    let right = view_direction.cross(reference_up).normalize();
    let up = right.cross(view_direction).normalize();
    let texel_m = 514.0 / f64::from(settings.resolution.max(1));
    let light_x = center.dot(right);
    let light_y = center.dot(up);
    let snapped_x = (light_x / texel_m).round() * texel_m;
    let snapped_y = (light_y / texel_m).round() * texel_m;
    let snapped_center = center + right * (snapped_x - light_x) + up * (snapped_y - light_y);
    let eye = snapped_center + to_body * 512.0;
    let view = glam::Mat4::look_at_rh(eye.as_vec3(), snapped_center.as_vec3(), up.as_vec3());
    let projection = glam::Mat4::orthographic_rh(-257.0, 257.0, -257.0, 257.0, 128.0, 896.0);
    let matrix = projection * view;
    let receiver_radius = settings.receiver_radius_m;
    let uniform = ShadowUniform {
        light_view_projection: matrix.to_cols_array_2d(),
        receiver_center_relative_radius: [
            center_relative[0] as f32,
            center_relative[1] as f32,
            center_relative[2] as f32,
            receiver_radius,
        ],
        texel_depth_range: [
            1.0 / settings.resolution.max(1) as f32,
            1.0 / settings.resolution.max(1) as f32,
            128.0,
            896.0,
        ],
        bias_fade: [
            settings.constant_bias_m,
            settings.slope_bias_m,
            settings.max_bias_m,
            settings.fade_start_m,
        ],
        flags: [1, source.id(), 0, 0],
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

pub(super) fn geometry_bounds(geometry: &SectorGeometry) -> Option<ShadowBounds> {
    let mut bounds = EmptyBounds::new();
    for map_box in &geometry.boxes {
        let half = map_box.size.map(|value| f64::from(value.abs()) * 0.5);
        let center = map_box.center.map(f64::from);
        bounds.include(std::array::from_fn(|axis| center[axis] - half[axis]));
        bounds.include(std::array::from_fn(|axis| center[axis] + half[axis]));
    }
    for triangle in &geometry.triangles {
        let points = triangle.precise_vertices.map_or_else(
            || triangle.vertices.map(|point| point.map(f64::from)),
            |vertices| vertices.map(|point| point.map(|value| value as f64 / 1000.0)),
        );
        for point in points {
            bounds.include(point);
        }
    }
    bounds.finish()
}

struct EmptyBounds {
    minimum: [f64; 3],
    maximum: [f64; 3],
}

impl EmptyBounds {
    fn new() -> Self {
        Self {
            minimum: [f64::INFINITY; 3],
            maximum: [f64::NEG_INFINITY; 3],
        }
    }

    fn include(&mut self, point: [f64; 3]) {
        for (axis, coordinate) in point.into_iter().enumerate() {
            self.minimum[axis] = self.minimum[axis].min(coordinate);
            self.maximum[axis] = self.maximum[axis].max(coordinate);
        }
    }

    fn finish(self) -> Option<ShadowBounds> {
        (0..3)
            .all(|axis| self.minimum[axis].is_finite() && self.maximum[axis].is_finite())
            .then_some(ShadowBounds {
                minimum: self.minimum,
                maximum: self.maximum,
            })
    }
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
