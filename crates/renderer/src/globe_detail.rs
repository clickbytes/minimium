//! Local globe region geometry and its GPU detail tile resources.
use crate::GlobeVertex;
use std::f64::consts::{PI, TAU};

#[derive(Clone, Copy)]
pub struct Region {
    pub u_start: f64,
    pub v_start: f64,
    pub u_span: f64,
    pub v_span: f64,
}

impl Region {
    pub const FULL: Self = Self {
        u_start: 0.0,
        v_start: 0.0,
        u_span: 1.0,
        v_span: 1.0,
    };

    pub fn around(direction: glam::Vec3, zoom: f32) -> Self {
        let latitude = f64::from(direction.y).asin();
        let u = (f64::from(direction.z.atan2(direction.x)) / TAU + 0.5).rem_euclid(1.0);
        let v = 0.5 - latitude / PI;
        let horizon = (1.0 / f64::from(zoom)).acos();
        let v_span = (3.0 * horizon / PI).clamp(0.04, 0.5);
        let u_span = (3.0 * horizon / (TAU * latitude.cos().abs().max(0.25))).clamp(0.04, 0.5);
        Self {
            u_start: u - u_span * 0.5,
            v_start: (v - v_span * 0.5).clamp(0.0, 1.0 - v_span),
            u_span,
            v_span,
        }
    }

    pub fn covers(self, wanted: Self) -> bool {
        let u_shift = ((wanted.u_start - self.u_start + 0.5).rem_euclid(1.0) - 0.5).abs();
        let v_shift = (wanted.v_start - self.v_start).abs();
        u_shift < self.u_span * 0.15
            && v_shift < self.v_span * 0.15
            && (wanted.u_span / self.u_span - 1.0).abs() < 0.2
            && (wanted.v_span / self.v_span - 1.0).abs() < 0.2
    }
}

pub struct DetailGpu {
    pub bind_group: wgpu::BindGroup,
    pub vertices: wgpu::Buffer,
    pub vertex_count: u32,
    pub bytes: u64,
    pub region: Region,
}

pub fn patch_vertices(radius: f32, region: Region) -> Vec<GlobeVertex> {
    const COLUMNS: u32 = 256;
    const ROWS: u32 = 128;
    let mut vertices = Vec::with_capacity((COLUMNS * ROWS * 6) as usize);
    let point = |x: u32, y: u32| {
        let u = f64::from(x) / f64::from(COLUMNS);
        let v = f64::from(y) / f64::from(ROWS);
        let lon = ((region.u_start + u * region.u_span) * 360.0 - 180.0).to_radians();
        let lat = (90.0 - (region.v_start + v * region.v_span) * 180.0).to_radians();
        let r = f64::from(radius) * 1.00001;
        GlobeVertex {
            position: [
                (r * lat.cos() * lon.cos()) as f32,
                (r * lat.sin()) as f32,
                (r * lat.cos() * lon.sin()) as f32,
            ],
            uv: [u as f32, v as f32],
        }
    };
    for y in 0..ROWS {
        for x in 0..COLUMNS {
            let (a, b, c, d) = (
                point(x, y),
                point(x + 1, y),
                point(x, y + 1),
                point(x + 1, y + 1),
            );
            vertices.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    vertices
}
