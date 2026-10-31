//! Canonical cloud microstructure. Coordinates are local to a physical lobe,
//! so detail follows advection, growth and teleportation instead of the camera.
//! Keep constants and integer hashing aligned with cloud_detail.wgsl.
use glam::DVec3;

pub(super) fn kernel(q: DVec3, seed: u32) -> f64 {
    let radial = 1.0 - q.length_squared();
    if radial <= 0.0 {
        return 0.0;
    }
    let coarse = noise(q * 3.5 + DVec3::new(7.13, 13.7, 19.2), seed);
    let fine = noise(q * 11.0 + DVec3::new(37.0, 11.0, 3.0), seed ^ 0x9e3779b9);
    // Erode the smooth ellipsoid boundary, without extending its conservative bounds.
    let carved = (radial + (coarse - 0.5) * 0.26 + (fine - 0.5) * 0.08).max(0.0);
    let edge = (radial / 0.035).clamp(0.0, 1.0);
    let edge = edge * edge * (3.0 - 2.0 * edge);
    carved * carved * edge * (0.72 + 0.56 * fine)
}

fn noise(p: DVec3, seed: u32) -> f64 {
    let floor = p.floor();
    let cell = [floor.x as i32, floor.y as i32, floor.z as i32];
    let t = p - floor;
    let t = t * t * (DVec3::splat(3.0) - t * 2.0);
    let mut value = 0.0;
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let weight = if x == 0 { 1.0 - t.x } else { t.x }
                    * if y == 0 { 1.0 - t.y } else { t.y }
                    * if z == 0 { 1.0 - t.z } else { t.z };
                value += hash(
                    [cell[0] + x, cell[1] + y, cell[2] + z],
                    seed,
                ) * weight;
            }
        }
    }
    value
}

fn hash(cell: [i32; 3], seed: u32) -> f64 {
    let mut value = (cell[0] as u32).wrapping_mul(0x8da6b343)
        ^ (cell[1] as u32).wrapping_mul(0xd8163841)
        ^ (cell[2] as u32).wrapping_mul(0xcb1ab31fu32)
        ^ seed;
    value = (value ^ (value >> 16)).wrapping_mul(0x7feb352d);
    value = (value ^ (value >> 15)).wrapping_mul(0x846ca68b);
    value ^= value >> 16;
    f64::from(value >> 8) / 16777216.0
}
