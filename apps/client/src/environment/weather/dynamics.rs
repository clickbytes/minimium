//! Smooth spherical fronts and bounded, reproducible atmospheric circulation.
pub(super) fn noise(p: [f64; 3], seed: u32) -> f64 {
    let c = p.map(|v| v.floor() as i32);
    let w = p.map(|v| {
        let f = v - v.floor();
        f * f * (3.0 - 2.0 * f)
    });
    let mut value = 0.0;
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let mut h = ((c[0] + x) as u32).wrapping_mul(0x8da6b343)
                    ^ ((c[1] + y) as u32).wrapping_mul(0xd8163841)
                    ^ ((c[2] + z) as u32).wrapping_mul(0xcb1ab31f)
                    ^ seed;
                h = (h ^ (h >> 16)).wrapping_mul(0x7feb352d);
                h = (h ^ (h >> 15)).wrapping_mul(0x846ca68b);
                h ^= h >> 16;
                let weight = [x, y, z]
                    .iter()
                    .enumerate()
                    .map(|(i, n)| if *n == 0 { 1.0 - w[i] } else { w[i] })
                    .product::<f64>();
                value += f64::from(h >> 8) / 16777216.0 * weight;
            }
        }
    }
    value
}

pub(super) fn fronts(lat: f64, lon: f64, days: f64, seed: u128) -> [f64; 3] {
    super::systems::forcing(lat, lon, days, seed)
}
