use super::{
    voxels::{Voxels, COVERAGE},
    Face,
};
use crate::environment::weather::Cloud;

/// Signed endpoint differences expose a face as its neighbour disappears.
/// The shader interpolates before clamping, including sign changes in a window.
pub(super) fn build(cloud: Cloud, cells: &Voxels, limit: usize) -> Option<Vec<Face>> {
    let mut faces = Vec::new();
    let dims = cells.dims;
    for axis in 0..3 {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        for slice in 0..=dims[axis] {
            for sign in [-1, 1] {
                let mut mask = vec![[0_i8; 2]; dims[u] * dims[v]];
                for j in 0..dims[v] {
                    for i in 0..dims[u] {
                        let mut p = [0; 3];
                        p[u] = i;
                        p[v] = j;
                        p[axis] = slice.saturating_sub(1);
                        let a = if slice > 0 { cells.at(p) } else { [0; 2] };
                        p[axis] = slice.min(dims[axis] - 1);
                        let b = if slice < dims[axis] {
                            cells.at(p)
                        } else {
                            [0; 2]
                        };
                        let delta = std::array::from_fn(|t| (a[t] as i8 - b[t] as i8) * sign);
                        if delta.iter().any(|d| *d > 0) {
                            mask[j * dims[u] + i] = delta;
                        }
                    }
                }
                for j in 0..dims[v] {
                    let mut i = 0;
                    while i < dims[u] {
                        let value = mask[j * dims[u] + i];
                        if value == [0; 2] {
                            i += 1;
                            continue;
                        }
                        let mut width = 1;
                        while i + width < dims[u] && mask[j * dims[u] + i + width] == value {
                            width += 1;
                        }
                        let mut height = 1;
                        while j + height < dims[v]
                            && (0..width).all(|k| mask[(j + height) * dims[u] + i + k] == value)
                        {
                            height += 1;
                        }
                        if faces.len() >= limit {
                            return None;
                        }
                        let mut center = [0.0; 4];
                        center[axis] = slice as f32;
                        center[u] = i as f32 + width as f32 * 0.5;
                        center[v] = j as f32 + height as f32 * 0.5;
                        for coordinate in &mut center[..3] {
                            *coordinate *= cells.cell as f32;
                        }
                        center[0] -= cloud.size_m.x as f32 * 0.5;
                        center[2] -= cloud.size_m.z as f32 * 0.5;
                        let mut du = [0.0; 4];
                        let mut dv = [0.0; 4];
                        du[u] = width as f32 * cells.cell as f32 * 0.5 * f32::from(sign);
                        dv[v] = height as f32 * cells.cell as f32 * 0.5;
                        du[3] = f32::from(value[0]) / COVERAGE;
                        dv[3] = f32::from(value[1]) / COVERAGE;
                        faces.push(Face {
                            center,
                            u: du,
                            v: dv,
                        });
                        for row in j..j + height {
                            for col in i..i + width {
                                mask[row * dims[u] + col] = [0; 2];
                            }
                        }
                        i += width;
                    }
                }
            }
        }
    }
    Some(faces)
}

/// Emergency six-face representation of the same bounded volume. Coverage is
/// derived from its sampled field, never from a new seed or a new population.
pub(super) fn envelope(cloud: Cloud, coverage: [f32; 2]) -> Vec<Face> {
    let mut faces = Vec::with_capacity(6);
    if coverage == [0.0; 2] {
        return faces;
    }
    let size = cloud.size_m.as_vec3().to_array();
    for axis in 0..3 {
        let u = (axis + 1) % 3;
        let v = (axis + 2) % 3;
        for sign in [-1.0, 1.0] {
            let mut center = [0.0, size[1] * 0.5, 0.0, 0.0];
            center[axis] += size[axis] * 0.5 * sign;
            let mut du = [0.0; 4];
            let mut dv = [0.0; 4];
            du[u] = size[u] * 0.5 * sign;
            dv[v] = size[v] * 0.5;
            du[3] = coverage[0];
            dv[3] = coverage[1];
            faces.push(Face {
                center,
                u: du,
                v: dv,
            });
        }
    }
    faces
}
