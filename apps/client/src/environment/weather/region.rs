//! Tangent-plane weather patch: no longitude seam or polar singularity.
use super::simulated::{HEIGHT, WIDTH};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Region(pub(super) [f64; 4]);

impl Region {
    pub(super) const GLOBAL: Self = Self([0.0; 4]);

    pub(super) fn around(latitude: f64, longitude: f64) -> Self {
        // Stable centres; the inner half of overlapping patches has full detail.
        let lat = latitude.round().to_radians();
        let lon_step = (1.0 / lat.cos().abs().max(0.05)).min(360.0);
        let lon = (longitude / lon_step).round() * lon_step;
        let lon = lon.to_radians();
        Self([
            lat.cos() * lon.cos(),
            lat.sin(),
            lat.cos() * lon.sin(),
            0.035,
        ])
    }

    fn basis(self) -> ([f64; 3], [f64; 3]) {
        let [x, y, z, _] = self.0;
        let n = x.hypot(z);
        let east = if n > 1e-8 {
            [-z / n, 0.0, x / n]
        } else {
            [0.0, 0.0, 1.0]
        };
        let north = [y * east[2], z * east[0] - x * east[2], -y * east[0]];
        // east cross up points south; negate to obtain north.
        (east, north.map(|v| -v))
    }

    pub(super) fn coordinates(self, x: usize, y: usize) -> (f64, f64) {
        if self.0[3] == 0.0 {
            return (
                (0.5 - (y as f64 + 0.5) / HEIGHT as f64) * std::f64::consts::PI,
                ((x as f64 + 0.5) / WIDTH as f64 - 0.5) * std::f64::consts::TAU,
            );
        }
        let (east, north) = self.basis();
        let u = ((x as f64 + 0.5) / WIDTH as f64 * 2.0 - 1.0) * self.0[3];
        let v = (1.0 - (y as f64 + 0.5) / HEIGHT as f64 * 2.0) * self.0[3];
        let p: [f64; 3] = std::array::from_fn(|i| self.0[i] + east[i] * u + north[i] * v);
        let length = p.iter().map(|v| v * v).sum::<f64>().sqrt();
        ((p[1] / length).asin(), p[2].atan2(p[0]))
    }

    pub(super) fn uv_weight(self, lat: f64, lon: f64) -> ([f64; 2], f32) {
        let p = [lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin()];
        let dot = |a: [f64; 3]| a.iter().zip(p).map(|(a, b)| a * b).sum::<f64>();
        let facing = dot([self.0[0], self.0[1], self.0[2]]);
        if facing <= 0.0 || self.0[3] == 0.0 {
            return ([0.5; 2], 0.0);
        }
        let (east, north) = self.basis();
        let uv = [
            0.5 + dot(east) / facing / self.0[3] * 0.5,
            0.5 - dot(north) / facing / self.0[3] * 0.5,
        ];
        let edge = ((uv[0] - 0.5).abs().max((uv[1] - 0.5).abs()) - 0.25) * 4.0;
        let t = edge.clamp(0.0, 1.0);
        (uv, (1.0 - t * t * (3.0 - 2.0 * t)) as f32)
    }
}
