//! World-anchored exterior ground heights from the meshes selected for this scene.
use crate::{prepared_sector::PreparedSector, shadow_geometry::ShadowBounds};
use glam::{DVec2, DVec3};
use half::f16;
use std::sync::Arc;

pub const EDGE: u32 = 33;
const LAST: f64 = (EDGE - 1) as f64;

pub struct FogRelief {
    /// First cube-face sample (u,v), angular grid step, signed dominant axis (1..3).
    pub(crate) mapping: [f32; 4],
    /// Split height above the sea datum in game metres, plus valid coverage.
    pub(crate) pixels: Vec<[f16; 4]>,
}

struct Projection {
    axis: usize,
    sign: f64,
    first: DVec2,
    step: f64,
}

impl Projection {
    fn new(eye: DVec3, radius: f64) -> Self {
        let (axis, sign) = face_axes(eye);
        let uv = DVec2::new(eye[(axis + 1) % 3], eye[(axis + 2) % 3]) / eye[axis].abs();
        // At most 16 game metres per sample. Recenter by eight whole cells so
        // the interior of adjacent tiles samples exactly the same world rays.
        let step = (16.0 / radius).min(1.0 / 1024.0);
        let first = ((uv / (8.0 * step)).round() * 8.0 - LAST * 0.5) * step;
        Self {
            axis,
            sign,
            first,
            step,
        }
    }

    fn intersects(&self, bounds: ShadowBounds) -> bool {
        let dominant = |a: [f64; 3]| a[self.axis] * self.sign;
        let a = dominant(bounds.minimum);
        let b = dominant(bounds.maximum);
        let far = a.max(b);
        if far <= 0.0 {
            return false;
        }
        let near = a.min(b).max(0.0);
        (0..2).all(|i| {
            let axis = (self.axis + i + 1) % 3;
            let low = self.first[i];
            let high = low + self.step * LAST;
            bounds.maximum[axis] >= (low * near).min(low * far)
                && bounds.minimum[axis] <= (high * near).max(high * far)
        })
    }
}

pub(crate) fn face_axes(eye: DVec3) -> (usize, f64) {
    let axis = eye.abs().max_position();
    (axis, eye[axis].signum())
}

impl FogRelief {
    /// A dense scene over budget falls back atomically to the regional weather grid.
    pub fn build(
        patches: &[Arc<PreparedSector>],
        origin: [f32; 3],
        eye: [f32; 3],
        sea_radius_m: f64,
    ) -> Option<Self> {
        let eye = DVec3::from_array(eye.map(f64::from));
        if eye.length_squared() < 1.0 || eye.length() - sea_radius_m > 20_000.0 {
            return None;
        }
        let grid = Projection::new(eye, sea_radius_m);
        let origin = DVec3::from_array(origin.map(f64::from));
        let mut depth = vec![0.0_f64; (EDGE * EDGE) as usize];
        let mut triangles_left = 65_536_usize;
        let mut samples_left = 262_144_usize;
        for patch in patches {
            if !patch.bounds.is_some_and(|b| grid.intersects(b)) {
                continue;
            }
            let triangles = patch.surface.chunks_exact(3);
            triangles_left = triangles_left.checked_sub(triangles.len())?;
            for triangle in triangles {
                let points: [DVec3; 3] = std::array::from_fn(|i| {
                    origin + DVec3::from_array(triangle[i].position.map(f64::from))
                });
                let d = points.map(|p| p[grid.axis] * grid.sign);
                if d.iter().any(|v| *v <= 0.0) {
                    continue;
                }
                let uv: [DVec2; 3] = std::array::from_fn(|i| {
                    let p = points[i];
                    let face = DVec2::new(p[(grid.axis + 1) % 3], p[(grid.axis + 2) % 3]) / d[i];
                    (face - grid.first) / grid.step
                });
                let ab = uv[1] - uv[0];
                let ac = uv[2] - uv[0];
                let area = ab.perp_dot(ac);
                if area.abs() < 1e-10 {
                    continue;
                }
                let lo = uv[0].min(uv[1]).min(uv[2]).ceil().max(DVec2::ZERO);
                let hi = uv[0].max(uv[1]).max(uv[2]).floor().min(DVec2::splat(LAST));
                if lo.cmpgt(hi).any() {
                    continue;
                }
                let size = (hi - lo + 1.0).as_uvec2();
                samples_left = samples_left.checked_sub((size.x * size.y) as usize)?;
                for y in lo.y as u32..=hi.y as u32 {
                    for x in lo.x as u32..=hi.x as u32 {
                        let ap = DVec2::new(f64::from(x), f64::from(y)) - uv[0];
                        let b = ap.perp_dot(ac) / area;
                        let c = ab.perp_dot(ap) / area;
                        let a = 1.0 - b - c;
                        if a.min(b).min(c) >= -1e-9 {
                            // Perspective-correct cube-face depth. Keep the highest
                            // exterior surface in columns with caves or overhangs.
                            let distance = (a / d[0] + b / d[1] + c / d[2]).recip();
                            let cell = &mut depth[(y * EDGE + x) as usize];
                            *cell = cell.max(distance);
                        }
                    }
                }
            }
        }
        if depth.iter().all(|d| *d == 0.0) {
            return None;
        }
        let positions: Vec<_> = depth
            .iter()
            .enumerate()
            .map(|(i, &d)| {
                let uv = grid.first
                    + DVec2::new((i as u32 % EDGE) as f64, (i as u32 / EDGE) as f64) * grid.step;
                let mut p = DVec3::ZERO;
                p[grid.axis] = grid.sign * d;
                p[(grid.axis + 1) % 3] = uv.x * d;
                p[(grid.axis + 2) % 3] = uv.y * d;
                p
            })
            .collect();
        let pixels = positions
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if depth[i] == 0.0 {
                    return [f16::ZERO; 4];
                }
                let height = p.length() - sea_radius_m;
                let high = f16::from_f64(height / 32.0);
                [
                    high,
                    f16::from_f64(height - high.to_f64() * 32.0),
                    f16::ONE,
                    f16::from_f64(blocked_sky(i, &positions)),
                ]
            })
            .collect();
        Some(Self {
            mapping: [
                grid.first.x as f32,
                grid.first.y as f32,
                grid.step as f32,
                (grid.sign * (grid.axis + 1) as f64) as f32,
            ],
            pixels,
        })
    }
}

// For a radial receiver the cosine-weighted blocked fraction in each azimuth is
// sin²(horizon elevation). Missing columns contribute no inferred obstruction.
fn blocked_sky(index: usize, positions: &[DVec3]) -> f64 {
    let origin = positions[index];
    let up = origin.normalize();
    let edge = EDGE as i32;
    let mut blocked = 0.0;
    for (dx, dy) in [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ] {
        let mut x = index as i32 % edge + dx;
        let mut y = index as i32 / edge + dy;
        let mut highest = 0.0_f64;
        while (0..edge).contains(&x) && (0..edge).contains(&y) {
            let point = positions[(y * edge + x) as usize];
            if point != DVec3::ZERO {
                let delta = point - origin;
                highest =
                    highest.max(delta.dot(up).max(0.0).powi(2) / delta.length_squared().max(1e-12));
            }
            x += dx;
            y += dy;
        }
        blocked += highest;
    }
    blocked / 8.0
}
