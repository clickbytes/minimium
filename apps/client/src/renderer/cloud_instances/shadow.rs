//! Bounded moment cache, generated only from canonical ellipsoid columns.
use crate::environment::weather::clouds::{moments, types::CloudColumnSample, CloudFieldSnapshot};
use crate::environment::weather::wind::{basis, EARTH_RADIUS};
use glam::DVec3;
use std::sync::Arc;

pub(in crate::renderer) const SIZE: u32 = 32;
pub(super) const BYTES: u64 = SIZE as u64 * SIZE as u64 * 4 * 8;

pub(in crate::renderer) struct Cache {
    pub(in crate::renderer) pixels: Vec<u8>,
    samples: [Vec<CloudColumnSample>; 4],
    region: [f32; 4],
}

pub(super) fn build(fields: &[Arc<CloudFieldSnapshot>; 2], region: [f32; 4]) -> Cache {
    let mut cache = Cache {
        pixels: vec![0; BYTES as usize],
        samples: std::array::from_fn(|_| Vec::with_capacity((SIZE * SIZE) as usize)),
        region,
    };
    let center = DVec3::new(
        f64::from(region[0]),
        f64::from(region[1]),
        f64::from(region[2]),
    );
    let (east, north) = basis(center);
    for patch in 0..2 {
        if patch == 1 && region[3] <= 0.0 {
            cache.samples[2] = cache.samples[0].clone();
            cache.samples[3] = cache.samples[1].clone();
            let plane = (SIZE * SIZE * 4) as usize;
            cache.pixels.copy_within(0..plane * 2, plane * 2);
            cache.pixels.copy_within(plane * 4..plane * 6, plane * 6);
            continue;
        }
        for y in 0..SIZE {
            for x in 0..SIZE {
                let u = (f64::from(x) + 0.5) / f64::from(SIZE);
                let v = (f64::from(y) + 0.5) / f64::from(SIZE);
                let up = if patch == 1 && region[3] > 0.0 {
                    (center
                        + east * ((u * 2.0 - 1.0) * f64::from(region[3]))
                        + north * ((1.0 - v * 2.0) * f64::from(region[3])))
                    .normalize()
                } else {
                    let lon = (u - 0.5) * std::f64::consts::TAU;
                    let lat = (0.5 - v) * std::f64::consts::PI;
                    DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
                };
                let footprint = if patch == 0 {
                    EARTH_RADIUS * 3.0 / f64::from(SIZE)
                } else {
                    EARTH_RADIUS * f64::from(region[3]) / f64::from(SIZE)
                };
                for (time, field) in fields.iter().enumerate() {
                    let sample = moments::area(field, up, footprint);
                    let offset =
                        ((patch * 2 + time) * (SIZE * SIZE) as usize + (y * SIZE + x) as usize) * 4;
                    for layer in 0..3 {
                        let opacity =
                            sample.coverage[layer] * (1.0 - (-sample.optical_depth[layer]).exp());
                        cache.pixels[offset + layer] = (opacity * 255.0).round() as u8;
                        cache.pixels[offset + (SIZE * SIZE * 16) as usize + layer] =
                            (sample.top_m[layer] / 20000.0 * 255.0)
                                .clamp(0.0, 255.0)
                                .round() as u8;
                    }
                    cache.pixels[offset + 3] =
                        (sample.rain_mm_h / 32.0 * 255.0).clamp(0.0, 255.0).round() as u8;
                    cache.samples[patch * 2 + time].push(sample);
                }
            }
        }
    }
    cache
}

impl Cache {
    pub(in crate::renderer) fn sample(&self, up: DVec3, time: usize) -> CloudColumnSample {
        let uv = [
            (up.z.atan2(up.x) / std::f64::consts::TAU + 0.5).rem_euclid(1.0),
            0.5 - up.y.asin() / std::f64::consts::PI,
        ];
        let center = DVec3::new(
            f64::from(self.region[0]),
            f64::from(self.region[1]),
            f64::from(self.region[2]),
        );
        let (east, north) = basis(center);
        let facing = up.dot(center);
        let local = if self.region[3] > 0.0 && facing > 0.0 {
            [
                0.5 + up.dot(east) / facing / f64::from(self.region[3]) * 0.5,
                0.5 - up.dot(north) / facing / f64::from(self.region[3]) * 0.5,
            ]
        } else {
            [-1.0; 2]
        };
        let weight = 1.0
            - crate::environment::weather::wind::smooth(
                0.25,
                0.5,
                (local[0] - 0.5).abs().max((local[1] - 0.5).abs()),
            );
        blend(
            self.read(uv, time, false),
            self.read(local, time + 2, true),
            weight,
        )
    }

    fn read(&self, uv: [f64; 2], index: usize, local: bool) -> CloudColumnSample {
        let x = uv[0] * f64::from(SIZE) - 0.5;
        let y = uv[1] * f64::from(SIZE) - 0.5;
        let at = |x: i32, y: i32| {
            let x = if local {
                x.clamp(0, SIZE as i32 - 1)
            } else {
                x.rem_euclid(SIZE as i32)
            };
            self.samples[index][(y.clamp(0, SIZE as i32 - 1) * SIZE as i32 + x) as usize]
        };
        let a = blend(
            at(x.floor() as i32, y.floor() as i32),
            at(x.floor() as i32 + 1, y.floor() as i32),
            x - x.floor(),
        );
        let b = blend(
            at(x.floor() as i32, y.floor() as i32 + 1),
            at(x.floor() as i32 + 1, y.floor() as i32 + 1),
            x - x.floor(),
        );
        blend(a, b, y - y.floor())
    }
}

fn blend(a: CloudColumnSample, b: CloudColumnSample, t: f64) -> CloudColumnSample {
    let array = |a: [f64; 3], b: [f64; 3]| std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t);
    let coverage = array(a.coverage, b.coverage);
    let occupied = |av: [f64; 3], bv: [f64; 3]| {
        std::array::from_fn(|i| {
            if coverage[i] < 1e-8 {
                av[i] + (bv[i] - av[i]) * t
            } else {
                (av[i] * a.coverage[i] * (1.0 - t) + bv[i] * b.coverage[i] * t) / coverage[i]
            }
        })
    };
    let transmission = occupied(
        a.optical_depth.map(|v| (-v).exp()),
        b.optical_depth.map(|v| (-v).exp()),
    );
    CloudColumnSample {
        coverage,
        optical_depth: transmission.map(|v| -v.clamp(1e-8, 1.0).ln()),
        base_m: occupied(a.base_m, b.base_m),
        top_m: occupied(a.top_m, b.top_m),
        rain_mm_h: a.rain_mm_h + (b.rain_mm_h - a.rain_mm_h) * t,
        storm: a.storm + (b.storm - a.storm) * t,
        storm_id: if t < 0.5 { a.storm_id } else { b.storm_id },
    }
}

pub(super) fn texture(device: &wgpu::Device) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("canonical cloud column cache"),
        size: wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 8,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}
