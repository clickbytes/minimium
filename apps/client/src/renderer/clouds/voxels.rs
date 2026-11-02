//! Sparse 8^3 bricks. Uniform dyadic level within a cloud avoids LOD seams.
use crate::environment::weather::{Cloud, DensityField};
use glam::DVec3;

const SIDE: usize = 8;
const CELLS: usize = SIDE * SIDE * SIDE;
pub(super) const COVERAGE: f32 = 15.0;
pub(super) type Pair = [u8; 2];

pub(super) struct Voxels {
    pub dims: [usize; 3],
    pub cell: f64,
    pub bricks: usize,
    pub mean: [f32; 2],
    blocks: Vec<Option<Box<[Pair; CELLS]>>>,
    counts: [usize; 3],
}

impl Voxels {
    pub fn build(cloud: Cloud, level: u8, start: f64, step: f64) -> Self {
        let (dims, cell) = cloud.grid(level);
        let counts = dims.map(|n| n.div_ceil(SIDE));
        let mut result = Self {
            dims,
            cell,
            bricks: 0,
            mean: [0.0; 2],
            blocks: vec![None; counts.iter().product()],
            counts,
        };
        let fields = [
            DensityField::new(cloud, start),
            DensityField::new(cloud, start + step),
        ];
        for bz in 0..counts[2] {
            for by in 0..counts[1] {
                for bx in 0..counts[0] {
                    let mut data = [[0; 2]; CELLS];
                    let mut active = false;
                    for z in 0..SIDE {
                        for y in 0..SIDE {
                            for x in 0..SIDE {
                                let p = [bx * SIDE + x, by * SIDE + y, bz * SIDE + z];
                                if (0..3).any(|i| p[i] >= dims[i]) {
                                    continue;
                                }
                                let local = DVec3::new(
                                    p[0] as f64 + 0.5,
                                    p[1] as f64 + 0.5,
                                    p[2] as f64 + 0.5,
                                ) * cell
                                    - cloud.size_m * DVec3::new(0.5, 0.0, 0.5);
                                let pair = std::array::from_fn(|t| {
                                    let density = if level == cloud.min_level {
                                        // Area samples preserve narrow filaments on the coarsest grid.
                                        let mut sum = 0.0;
                                        for dz in [-0.25, 0.25] {
                                            for dx in [-0.25, 0.25] {
                                                sum += fields[t]
                                                    .sample(local + DVec3::new(dx, 0.0, dz) * cell);
                                            }
                                        }
                                        sum * 0.25
                                    } else {
                                        fields[t].sample(local)
                                    };
                                    (density * f64::from(COVERAGE)).round() as u8
                                });
                                result.mean[0] += f32::from(pair[0]);
                                result.mean[1] += f32::from(pair[1]);
                                active |= pair != [0; 2];
                                data[(z * SIDE + y) * SIDE + x] = pair;
                            }
                        }
                    }
                    if active {
                        let index = (bz * counts[1] + by) * counts[0] + bx;
                        result.blocks[index] = Some(Box::new(data));
                        result.bricks += 1;
                    }
                }
            }
        }
        let denominator = dims.iter().product::<usize>() as f32 * COVERAGE;
        result.mean = result.mean.map(|v| v / denominator);
        result
    }

    pub fn at(&self, p: [usize; 3]) -> Pair {
        let b = p.map(|n| n / SIDE);
        let index = (b[2] * self.counts[1] + b[1]) * self.counts[0] + b[0];
        self.blocks[index].as_ref().map_or([0; 2], |data| {
            data[((p[2] % SIDE) * SIDE + p[1] % SIDE) * SIDE + p[0] % SIDE]
        })
    }

    /// Aggregate dyadic blocks of the original lattice; keep physical bounds,
    /// mean coverage and IDs. No sampling or generation of replacement clouds.
    pub fn simplify(&mut self, stride: usize) {
        for z in (0..self.dims[2]).step_by(stride) {
            for y in (0..self.dims[1]).step_by(stride) {
                for x in (0..self.dims[0]).step_by(stride) {
                    let end = [x, y, z].map(|n| n + stride);
                    let end = std::array::from_fn::<_, 3, _>(|i| end[i].min(self.dims[i]));
                    let mut sum = [0_u32; 2];
                    let count = (end[0] - x) * (end[1] - y) * (end[2] - z);
                    for k in z..end[2] {
                        for j in y..end[1] {
                            for i in x..end[0] {
                                let value = self.at([i, j, k]);
                                for t in 0..2 {
                                    sum[t] += u32::from(value[t]);
                                }
                            }
                        }
                    }
                    let value = sum.map(|v| ((v as f64 / count as f64).round()) as u8);
                    for k in z..end[2] {
                        for j in y..end[1] {
                            for i in x..end[0] {
                                self.set([i, j, k], value);
                            }
                        }
                    }
                }
            }
        }
    }

    fn set(&mut self, p: [usize; 3], value: Pair) {
        let b = p.map(|n| n / SIDE);
        let index = (b[2] * self.counts[1] + b[1]) * self.counts[0] + b[0];
        if self.blocks[index].is_none() && value == [0; 2] {
            return;
        }
        let data = self.blocks[index].get_or_insert_with(|| Box::new([[0; 2]; CELLS]));
        data[((p[2] % SIDE) * SIDE + p[1] % SIDE) * SIDE + p[0] % SIDE] = value;
    }
}
