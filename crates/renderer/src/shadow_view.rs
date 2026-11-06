//! Bounded, camera-driven shadow coverage. No terrain sampling or GPU effects.
use crate::{RenderViewFrame, ShadowSettings};
use glam::{DMat4, DVec3};

#[derive(Clone, Copy, Debug)]
pub struct ShadowRegion {
    pub center_world: [f64; 3],
    pub settings: ShadowSettings,
}

impl ShadowRegion {
    /// Keep contact detail when close to resident ground. From flight/overview,
    /// fit the visible reference shell instead of a sphere around the player.
    pub fn for_view(
        view: RenderViewFrame,
        local_center: [f64; 3],
        ground_distance: f64,
        base: ShadowSettings,
        previous_radius: f32,
    ) -> Option<Self> {
        if !view.is_finite() || !ground_distance.is_finite() {
            return None;
        }
        let minimum = f64::from(base.receiver_radius_m);
        let eye = DVec3::from_array(view.camera_eye_world_m);
        let local = DVec3::from_array(local_center);
        let blend = if view.kind == crate::EnvironmentViewKind::Overview {
            1.0
        } else {
            let t = ((ground_distance - minimum * 0.25) / minimum).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        let points = surface_footprint(view);
        let (center, radius) = if blend > 0.0 && !points.is_empty() {
            let mut low = points[0];
            let mut high = low;
            for &point in &points {
                low = low.min(point);
                high = high.max(point);
            }
            let focus = (low + high) * 0.5;
            let center = local.lerp(focus, blend);
            let extent = points.iter().map(|p| p.distance(focus)).fold(0.0, f64::max);
            // Padding accommodates relief and asynchronous terrain selection;
            // the radial fade starts outside the fitted reference footprint.
            let radius = minimum + blend * (extent * 1.25 + ground_distance * 0.05);
            (center, radius)
        } else {
            (local, minimum)
        };
        if !center.is_finite() || !eye.is_finite() || !radius.is_finite() {
            return None;
        }
        // Quantized extents plus shrink hysteresis avoid continuous texel scaling.
        let requested = (radius.max(minimum).log2() * 4.0).ceil() / 4.0;
        let mut radius = requested.exp2() as f32;
        if previous_radius >= radius && previous_radius <= radius * 1.3 {
            radius = previous_radius;
        }
        let ratio = radius / base.receiver_radius_m;
        let settings = ShadowSettings {
            receiver_radius_m: radius,
            caster_radius_m: base.caster_radius_m * ratio,
            fade_start_m: radius * 0.9,
            constant_bias_m: base.constant_bias_m * ratio,
            slope_bias_m: base.slope_bias_m * ratio,
            max_bias_m: base.max_bias_m * ratio,
            ..base
        };
        Some(Self {
            center_world: center.to_array(),
            settings,
        })
    }
}

/// A small fixed grid includes the limb when a ray misses the planet. Sampling
/// only ray hits would collapse coverage when the disk is smaller than the view.
pub fn surface_footprint(view: RenderViewFrame) -> Vec<DVec3> {
    let eye = DVec3::from_array(view.camera_eye_world_m);
    let radius = view.game_planet_radius_m;
    if radius <= 0.0 || eye.length() < radius {
        return Vec::new();
    }
    let inverse =
        DMat4::from_cols_array_2d(&view.inverse_sky_view_projection.map(|c| c.map(f64::from)));
    let mut points = Vec::with_capacity(25);
    for y in [-1.0, -0.5, 0.0, 0.5, 1.0] {
        for x in [-1.0, -0.5, 0.0, 0.5, 1.0] {
            let ray = inverse
                .project_point3(DVec3::new(x, y, 1.0))
                .normalize_or_zero();
            if let Some(point) = shell_point(eye, ray, radius) {
                points.push(point);
            }
        }
    }
    points
}

fn shell_point(eye: DVec3, ray: DVec3, radius: f64) -> Option<DVec3> {
    let distance = eye.length();
    let b = eye.dot(ray);
    if b >= 0.0 || ray.length_squared() < 0.5 {
        return None;
    }
    let c = (distance - radius) * (distance + radius);
    let discriminant = b * b - c;
    if discriminant >= 0.0 {
        let t = c / (-b + discriminant.sqrt()).max(1e-9);
        return Some(eye + ray * t);
    }
    let up = eye / distance;
    let tangent = (ray - up * ray.dot(up)).normalize_or_zero();
    let cosine = radius / distance;
    Some((up * cosine + tangent * (1.0 - cosine * cosine).max(0.0).sqrt()) * radius)
}
