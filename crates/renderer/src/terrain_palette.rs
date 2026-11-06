//! Decode the validated 1x1 albedo mip once, avoiding per-fragment mean reads.
use minimium_assets::ktx2::Texture;

pub fn colors(texture: &Texture<'_>) -> [[f32; 4]; 6] {
    let mip = &texture.bytes[texture.levels.last().expect("validated mip chain").clone()];
    std::array::from_fn(|layer| {
        let rgb = if texture.vk_format == 132 {
            bc1(&mip[layer * 8..layer * 8 + 8])
        } else {
            std::array::from_fn(|i| f32::from(mip[layer * 4 + i]) / 255.0)
        };
        let linear = rgb.map(|v| {
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        });
        [linear[0], linear[1], linear[2], 1.0]
    })
}

fn bc1(block: &[u8]) -> [f32; 3] {
    let c0 = u16::from_le_bytes([block[0], block[1]]);
    let c1 = u16::from_le_bytes([block[2], block[3]]);
    let unpack = |c: u16| {
        let r = (c >> 11) & 31;
        let g = (c >> 5) & 63;
        let b = c & 31;
        [
            (r << 3) | (r >> 2),
            (g << 2) | (g >> 4),
            (b << 3) | (b >> 2),
        ]
        .map(|v| f32::from(v) / 255.0)
    };
    let a = unpack(c0);
    let b = unpack(c1);
    match block[4] & 3 {
        0 => a,
        1 => b,
        2 if c0 > c1 => std::array::from_fn(|i| (2.0 * a[i] + b[i]) / 3.0),
        3 if c0 > c1 => std::array::from_fn(|i| (a[i] + 2.0 * b[i]) / 3.0),
        2 => std::array::from_fn(|i| (a[i] + b[i]) / 2.0),
        _ => [0.0; 3],
    }
}
