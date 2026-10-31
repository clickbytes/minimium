//! Immutable atmospheric input, shared by physical clouds and presentation.
use crate::environment::weather::wind::basis;
pub(crate) type CloudColumn = [glam::Vec4; 8];
use crate::environment::weather::wind::smooth;
use crate::environment::weather::ATLAS_LAYERS;
use glam::{DVec3, Vec4};

pub(crate) struct Atlas {
    pub(crate) pixels: [Vec<u8>; ATLAS_LAYERS],
    pub(crate) region: [f32; 4],
    pub(crate) revision: u64,
    pub(crate) start_seconds: Option<f64>,
}

impl Atlas {
    pub(crate) fn column(&self, up: DVec3, blend: f32) -> CloudColumn {
        let uv = [
            (up.z.atan2(up.x) / std::f64::consts::TAU + 0.5).rem_euclid(1.0),
            0.5 - up.y.clamp(-1.0, 1.0).asin() / std::f64::consts::PI,
        ];
        let center = DVec3::new(
            f64::from(self.region[0]),
            f64::from(self.region[1]),
            f64::from(self.region[2]),
        );
        let facing = up.dot(center);
        let mut local = [0.5; 2];
        let mut weight = 0.0;
        if self.region[3] > 0.0 && facing > 0.0 {
            let (east, north) = basis(center);
            local = [
                0.5 + up.dot(east) / facing / f64::from(self.region[3]) * 0.5,
                0.5 - up.dot(north) / facing / f64::from(self.region[3]) * 0.5,
            ];
            let edge = (local[0] - 0.5).abs().max((local[1] - 0.5).abs());
            weight = 1.0 - smooth(0.25, 0.5, edge) as f32;
        }
        std::array::from_fn(|channel| {
            let layer = channel * 2;
            let global = self
                .read(uv, layer, false)
                .lerp(self.read(uv, layer + 1, false), blend);
            if weight <= 0.0 {
                return global;
            }
            let a = self.read(local, layer + ATLAS_LAYERS / 2, true);
            let b = self.read(local, layer + ATLAS_LAYERS / 2 + 1, true);
            global.lerp(a.lerp(b, blend), weight)
        })
    }

    fn read(&self, uv: [f64; 2], layer: usize, local: bool) -> Vec4 {
        let x = uv[0] * 512.0 - 0.5;
        let y = uv[1] * 256.0 - 0.5;
        let ix = x.floor() as i32;
        let iy = y.floor() as i32;
        let texel = |x: i32, y: i32| {
            let x = if local {
                x.clamp(0, 511)
            } else {
                x.rem_euclid(512)
            };
            let offset = (y.clamp(0, 255) * 512 + x) as usize * 4;
            Vec4::from_array(std::array::from_fn(|c| {
                f32::from(self.pixels[layer][offset + c]) / 255.0
            }))
        };
        let a = texel(ix, iy).lerp(texel(ix + 1, iy), (x - x.floor()) as f32);
        let b = texel(ix, iy + 1).lerp(texel(ix + 1, iy + 1), (x - x.floor()) as f32);
        a.lerp(b, (y - y.floor()) as f32)
    }
}
