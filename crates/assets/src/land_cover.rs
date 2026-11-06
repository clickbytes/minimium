//! Two-minute fractional ESA WorldCover grid. Missing cells retain a validity mask.
pub const WIDTH: usize = 10_800;
pub const HEIGHT: usize = 5_400;
pub const CHANNELS: usize = 7;
pub const BYTES: usize = 16 + WIDTH * HEIGHT * CHANNELS;

pub struct LandCover<'a> {
    bytes: &'a [u8],
}

impl<'a> LandCover<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, &'static str> {
        if bytes.len() != BYTES
            || &bytes[..8] != b"MMWC21\0\0"
            || u32::from_le_bytes(bytes[8..12].try_into().unwrap()) != WIDTH as u32
            || u32::from_le_bytes(bytes[12..16].try_into().unwrap()) != HEIGHT as u32
        {
            return Err("invalid WorldCover fractional grid");
        }
        Ok(Self { bytes })
    }

    /// Interpolate fractions across the antimeridian; return None only for no data.
    pub fn sample(&self, direction: [f64; 3]) -> Option<[f32; 6]> {
        let length = direction.iter().map(|v| v * v).sum::<f64>().sqrt();
        if !length.is_finite() || length < 1e-12 {
            return None;
        }
        let latitude = (direction[1] / length).clamp(-1.0, 1.0).asin();
        let longitude = direction[2].atan2(direction[0]);
        let x = (longitude / std::f64::consts::TAU + 0.5) * WIDTH as f64 - 0.5;
        let y = (0.5 - latitude / std::f64::consts::PI) * HEIGHT as f64 - 0.5;
        let mut result = [0.0; 6];
        let mut valid = 0.0;
        for dy in 0..2 {
            for dx in 0..2 {
                let column = (x.floor() as i64 + dx).rem_euclid(WIDTH as i64) as usize;
                let row = (y.floor() as i64 + dy).clamp(0, HEIGHT as i64 - 1) as usize;
                let i = 16 + (row * WIDTH + column) * CHANNELS;
                let weight = (if dx == 0 {
                    1.0 - (x - x.floor())
                } else {
                    x - x.floor()
                }) * (if dy == 0 {
                    1.0 - (y - y.floor())
                } else {
                    y - y.floor()
                });
                let weight = weight as f32 * f32::from(self.bytes[i + 6]) / 255.0;
                valid += weight;
                for (channel, v) in result.iter_mut().enumerate() {
                    *v += f32::from(self.bytes[i + channel]) / 255.0 * weight;
                }
            }
        }
        let sum: f32 = result.iter().sum();
        if valid < 1e-6 || sum < 1e-6 {
            return None;
        }
        for value in &mut result {
            *value /= sum;
        }
        Some(result)
    }
}
