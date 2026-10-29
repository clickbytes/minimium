use crate::{AddressedRandom, GenerationChannel};
use minimium_core::world::{CubeSampleKey, GenerationSpec, PlanetFace, Position3Mm, SectorKey};

const MAX_RADIUS_MM: u32 = 1_000_000_000;
const MIN_RADIUS_MM: u32 = 5_000;
const MAX_RELIEF_DIVISOR: u32 = 256;
const NOISE_Q16: i64 = 65_536;
const NOISE_LATTICE_STEP: i32 = PLANET_SAMPLE_COORDINATE_SCALE / 8;

use minimium_core::world::PLANET_SAMPLE_COORDINATE_SCALE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanetSurfaceSpec {
    radius_mm: u32,
    relief_mm: u32,
}

impl PlanetSurfaceSpec {
    /// Relief is capped at 1/256 of the base radius for the fixed smooth-noise
    /// lattice, keeping radial changes shallow enough for the generated approach.
    pub fn new(radius_mm: u32, relief_mm: u32) -> Option<Self> {
        if !(MIN_RADIUS_MM..=MAX_RADIUS_MM).contains(&radius_mm)
            || relief_mm > radius_mm / MAX_RELIEF_DIVISOR
        {
            return None;
        }
        Some(Self {
            radius_mm,
            relief_mm,
        })
    }

    pub const fn radius_mm(self) -> u32 {
        self.radius_mm
    }

    pub const fn relief_mm(self) -> u32 {
        self.relief_mm
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanetPoint {
    pub cube_sample: CubeSampleKey,
    pub height_mm: u32,
    pub position_mm: Position3Mm,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanetLandmarks {
    pub spawn: PlanetPoint,
    pub entrance: PlanetPoint,
    pub approach: [PlanetPoint; 9],
}

/// Fixed-point value-noise surface. The cube sample key makes face borders share
/// the same lattice coordinates; output positions are integral millimeters.
#[derive(Clone, Copy, Debug)]
pub struct PlanetTerrain {
    generation: GenerationSpec,
    surface: PlanetSurfaceSpec,
}

impl PlanetTerrain {
    pub fn new(generation: GenerationSpec, surface: PlanetSurfaceSpec) -> Option<Self> {
        if generation.position_quantum_mm() > surface.radius_mm() / 1024 {
            return None;
        }
        Some(Self {
            generation,
            surface,
        })
    }

    pub fn sample(self, cube_sample: CubeSampleKey) -> PlanetPoint {
        let offset = interpolate_noise(self.generation, cube_sample)
            * i64::from(self.surface.relief_mm)
            / 32_768;
        let height_mm = (i64::from(self.surface.radius_mm) + offset) as u32;
        let position_mm = self.position_at(cube_sample, height_mm);
        PlanetPoint {
            cube_sample,
            height_mm,
            position_mm,
        }
    }

    /// Safe deterministic spawn and entrance on one face, joined by sampled
    /// approach points. The relief bound keeps the route traversable.
    pub fn landmarks(self) -> PlanetLandmarks {
        let scale = PLANET_SAMPLE_COORDINATE_SCALE;
        let start = -(scale / 8);
        let step = scale / 32;
        let approach = std::array::from_fn(|index| {
            let sample = PlanetFace::PositiveY
                .cube_sample(0, start + index as i32 * step)
                .expect("fixed planet route samples lie inside the face");
            self.sample(sample)
        });
        PlanetLandmarks {
            spawn: approach[0],
            entrance: approach[8],
            approach,
        }
    }

    fn position_at(self, sample: CubeSampleKey, radius_mm: u32) -> Position3Mm {
        let [x, y, z] = sample.coordinates().map(i64::from);
        let length_squared = (x * x + y * y + z * z) as u64;
        let length = integer_sqrt(length_squared) as i64;
        let quantum = i64::from(self.generation.position_quantum_mm());
        let radius = i64::from(radius_mm);
        let quantize = |coordinate: i64| {
            let numerator = coordinate * radius;
            let rounded = divide_round_nearest(numerator, length);
            divide_round_nearest(rounded, quantum) * quantum
        };
        Position3Mm::new(quantize(x), quantize(y), quantize(z))
    }
}

fn interpolate_noise(generation: GenerationSpec, sample: CubeSampleKey) -> i64 {
    let [x, y, z] = sample.coordinates();
    let (x0, tx) = lattice_coordinate(x);
    let (y0, ty) = lattice_coordinate(y);
    let (z0, tz) = lattice_coordinate(z);
    let values = std::array::from_fn::<_, 8, _>(|index| {
        let dx = (index & 1) as i32;
        let dy = ((index >> 1) & 1) as i32;
        let dz = ((index >> 2) & 1) as i32;
        let cell = SectorKey::dungeon(x0 + dx, y0 + dy, z0 + dz);
        let word = AddressedRandom::new(generation, GenerationChannel::Terrain, cell).sample_u32(0);
        i64::from((word >> 16) as u16) - 32_768
    });
    let x00 = lerp(values[0], values[1], tx);
    let x10 = lerp(values[2], values[3], tx);
    let x01 = lerp(values[4], values[5], tx);
    let x11 = lerp(values[6], values[7], tx);
    let y0 = lerp(x00, x10, ty);
    let y1 = lerp(x01, x11, ty);
    lerp(y0, y1, tz)
}

fn lattice_coordinate(coordinate: i32) -> (i32, i64) {
    let cell = coordinate.div_euclid(NOISE_LATTICE_STEP);
    let remainder = coordinate.rem_euclid(NOISE_LATTICE_STEP);
    let fraction = i64::from(remainder) * NOISE_Q16 / i64::from(NOISE_LATTICE_STEP);
    (cell, smoothstep(fraction))
}

fn smoothstep(value: i64) -> i64 {
    let square = value * value / NOISE_Q16;
    square * (3 * NOISE_Q16 - 2 * value) / NOISE_Q16
}

fn lerp(start: i64, end: i64, amount_q16: i64) -> i64 {
    start + (end - start) * amount_q16 / NOISE_Q16
}

fn divide_round_nearest(numerator: i64, denominator: i64) -> i64 {
    if numerator < 0 {
        -((-numerator + denominator / 2) / denominator)
    } else {
        (numerator + denominator / 2) / denominator
    }
}

fn integer_sqrt(value: u64) -> u64 {
    if value < 2 {
        return value;
    }
    let mut current = value;
    let mut next = current.div_ceil(2);
    while next < current {
        current = next;
        next = (current + value / current) / 2;
    }
    current
}
