use crate::{AddressedRandom, GenerationChannel};
use minimium_core::world::{
    CubeSampleKey, GenerationSpec, PlanetDefinition, PlanetFace, Position3Mm, SectorKey,
};

const MAX_RADIUS_MM: u64 = 6_371_000_000;
const MIN_RADIUS_MM: u64 = 5_000;
const NOISE_Q16: i64 = 65_536;
const NOISE_LATTICE_STEP: i32 = PLANET_SAMPLE_COORDINATE_SCALE / 8;

use minimium_core::world::PLANET_SAMPLE_COORDINATE_SCALE;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanetSurfaceSpec {
    radius_mm: u64,
    relief_mm: u64,
}

impl PlanetSurfaceSpec {
    /// The displacement remains small relative to the planet radius.
    pub fn new(radius_mm: u64, relief_mm: u64) -> Option<Self> {
        if !(MIN_RADIUS_MM..=MAX_RADIUS_MM).contains(&radius_mm)
            || relief_mm > (radius_mm / 50).min(5_000_000)
        {
            return None;
        }
        Some(Self {
            radius_mm,
            relief_mm,
        })
    }

    pub const fn radius_mm(self) -> u64 {
        self.radius_mm
    }

    pub const fn relief_mm(self) -> u64 {
        self.relief_mm
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlanetPoint {
    pub cube_sample: CubeSampleKey,
    pub height_mm: u64,
    pub position_mm: Position3Mm,
    pub ocean: bool,
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
    definition: Option<PlanetDefinition>,
}

impl PlanetTerrain {
    pub fn new(generation: GenerationSpec, surface: PlanetSurfaceSpec) -> Option<Self> {
        if u64::from(generation.position_quantum_mm()) > surface.radius_mm() / 1024 {
            return None;
        }
        Some(Self {
            generation,
            surface,
            definition: None,
        })
    }

    pub fn with_definition(mut self, definition: PlanetDefinition) -> Self {
        self.definition = Some(definition);
        self
    }

    pub const fn radius_mm(self) -> u64 {
        self.surface.radius_mm()
    }

    pub fn sea_level_mm(self) -> i64 {
        self.definition
            .map_or(0, |definition| definition.sea_level_mm)
    }

    pub fn is_earth(self) -> bool {
        self.definition
            .is_some_and(|definition| definition.mode == 1)
    }

    pub fn sample(self, cube_sample: CubeSampleKey) -> PlanetPoint {
        let (offset, ocean) = if let Some(definition) =
            self.definition.filter(|definition| definition.mode == 1)
        {
            let elevation = crate::earth_elevation_mm(
                cube_sample.coordinates().map(f64::from),
                definition.height_scale_permille,
            );
            let ocean = elevation < definition.sea_level_mm;
            let detail = if ocean {
                0
            } else {
                let step = ((u64::from(definition.detail_wavelength_mm)
                    * PLANET_SAMPLE_COORDINATE_SCALE as u64
                    / self.surface.radius_mm)
                    .max(1)) as i32;
                interpolate_noise_at(
                    self.generation,
                    cube_sample,
                    step,
                    GenerationChannel::TerrainDetail,
                ) * i64::from(definition.detail_strength_mm)
                    / 32_768
            };
            (
                (elevation + detail)
                    .max(definition.sea_level_mm)
                    .min(i64::from(definition.mountain_limit_mm)),
                ocean,
            )
        } else if let Some(definition) = self.definition {
            let band_step = (PLANET_SAMPLE_COORDINATE_SCALE
                / definition.mountain_band_frequency.max(1) as i32)
                .max(1);
            let band = interpolate_noise_at(
                self.generation,
                cube_sample,
                band_step,
                GenerationChannel::TerrainBands,
            )
            .abs();
            let mountains = (band - 20_000).max(0) * self.surface.relief_mm as i64 / 12_768;
            let detail_step = ((u64::from(definition.detail_wavelength_mm)
                * PLANET_SAMPLE_COORDINATE_SCALE as u64
                / self.surface.radius_mm)
                .max(1)) as i32;
            let detail = interpolate_noise_at(
                self.generation,
                cube_sample,
                detail_step,
                GenerationChannel::TerrainDetail,
            ) * i64::from(definition.detail_strength_mm)
                / 32_768;
            let [x, y, z] = cube_sample.coordinates();
            let clearance = if y == PLANET_SAMPLE_COORDINATE_SCALE {
                let distance = x.abs().max(z.abs());
                let inner = PLANET_SAMPLE_COORDINATE_SCALE / 256;
                let outer = PLANET_SAMPLE_COORDINATE_SCALE / 64;
                i64::from((distance - inner).clamp(0, outer - inner)) * 1000
                    / i64::from(outer - inner)
            } else {
                1000
            };
            (
                ((mountains + detail) * i64::from(definition.height_scale_permille) * clearance
                    / 1_000_000)
                    .clamp(
                        -i64::from(definition.detail_strength_mm),
                        i64::from(definition.mountain_limit_mm),
                    ),
                false,
            )
        } else {
            (
                interpolate_noise(self.generation, cube_sample) * self.surface.relief_mm as i64
                    / 32_768,
                false,
            )
        };
        let height_mm = (self.surface.radius_mm as i64 + offset) as u64;
        let position_mm = self.position_at(cube_sample, height_mm);
        PlanetPoint {
            cube_sample,
            height_mm,
            position_mm,
            ocean,
        }
    }

    /// Safe deterministic spawn and entrance on one face, joined by sampled
    /// approach points. The relief bound keeps the route traversable.
    pub fn landmarks(self) -> PlanetLandmarks {
        let scale = PLANET_SAMPLE_COORDINATE_SCALE;
        let (u, start, step) = if self.is_earth() && self.generation.revision().get() >= 4 {
            // Near Bergen: 60.4 N, 5.3 E. This cube coordinate samples the
            // same GEBCO surface used by collision and sector rendering.
            let step = scale / 8192;
            (
                (i64::from(scale) * 566 / 1000) as i32,
                (i64::from(scale) * 527 / 10_000) as i32,
                step,
            )
        } else if self
            .definition
            .is_some_and(|definition| definition.mode == 1)
        {
            let jitter =
                (self.generation.seed().get() as i32).rem_euclid(scale / 1024) - scale / 2048;
            let step = scale / 4096;
            (
                scale / 1000 * 811 + jitter,
                scale / 1000 * 217 - 4 * step + jitter,
                step,
            )
        } else if self.generation.revision().get() >= 2 {
            (0, -(scale / 512), scale / 2048)
        } else {
            (0, -(scale / 8), scale / 32)
        };
        let approach = std::array::from_fn(|index| {
            let sample = PlanetFace::PositiveY
                .cube_sample(u, start + index as i32 * step)
                .expect("fixed planet route samples lie inside the face");
            self.sample(sample)
        });
        PlanetLandmarks {
            spawn: approach[0],
            entrance: approach[8],
            approach,
        }
    }

    fn position_at(self, sample: CubeSampleKey, radius_mm: u64) -> Position3Mm {
        let [x, y, z] = sample.coordinates().map(i64::from);
        let length_squared = (x * x + y * y + z * z) as u64;
        let length = integer_sqrt(length_squared) as i64;
        let quantum = i64::from(self.generation.position_quantum_mm());
        let radius = radius_mm as i64;
        let quantize = |coordinate: i64| {
            let numerator = coordinate * radius;
            let rounded = divide_round_nearest(numerator, length);
            divide_round_nearest(rounded, quantum) * quantum
        };
        Position3Mm::new(quantize(x), quantize(y), quantize(z))
    }
}

fn interpolate_noise(generation: GenerationSpec, sample: CubeSampleKey) -> i64 {
    interpolate_noise_at(
        generation,
        sample,
        NOISE_LATTICE_STEP,
        GenerationChannel::Terrain,
    )
}

fn interpolate_noise_at(
    generation: GenerationSpec,
    sample: CubeSampleKey,
    step: i32,
    channel: GenerationChannel,
) -> i64 {
    let [x, y, z] = sample.coordinates();
    let (x0, tx) = lattice_coordinate(x, step);
    let (y0, ty) = lattice_coordinate(y, step);
    let (z0, tz) = lattice_coordinate(z, step);
    let values = std::array::from_fn::<_, 8, _>(|index| {
        let dx = (index & 1) as i32;
        let dy = ((index >> 1) & 1) as i32;
        let dz = ((index >> 2) & 1) as i32;
        let cell = SectorKey::dungeon(x0 + dx, y0 + dy, z0 + dz);
        let word = AddressedRandom::new(generation, channel, cell).sample_u32(0);
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

fn lattice_coordinate(coordinate: i32, step: i32) -> (i32, i64) {
    let cell = coordinate.div_euclid(step);
    let remainder = coordinate.rem_euclid(step);
    let fraction = i64::from(remainder) * NOISE_Q16 / i64::from(step);
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
