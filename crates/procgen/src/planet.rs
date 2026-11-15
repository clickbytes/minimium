use crate::{AddressedRandom, GenerationChannel, GenerationError};
use minimium_world::world::{
    CubeSampleKey, GenerationSpec, PlanetDefinition, PlanetFace, Position3Mm, SectorKey,
};

const MAX_RADIUS_MM: u64 = 6_371_000_000;
const MIN_RADIUS_MM: u64 = 5_000;
const NOISE_Q16: i64 = 65_536;
const NOISE_LATTICE_STEP: i32 = PLANET_SAMPLE_COORDINATE_SCALE / 8;

use minimium_world::world::PLANET_SAMPLE_COORDINATE_SCALE;

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

/// Fixed-point value-noise surface. The cube sample key makes face borders share
/// the same lattice coordinates; output positions are integral millimeters.
#[derive(Clone, Debug)]
pub struct PlanetTerrain {
    generation: GenerationSpec,
    surface: PlanetSurfaceSpec,
    definition: Option<PlanetDefinition>,
    inputs: crate::TerrainInputs,
    pub(crate) material_manifest: Option<minimium_world::world::WorldManifest>,
    pub(crate) surface_recipe: crate::surface_habitat::SurfaceRecipe,
    relief_policy: crate::PlanetReliefSamplingPolicy,
}

impl PlanetTerrain {
    pub fn new(
        generation: GenerationSpec,
        surface: PlanetSurfaceSpec,
        definition: Option<PlanetDefinition>,
        inputs: crate::TerrainInputs,
    ) -> Result<Self, GenerationError> {
        let policy = inputs
            .resolved_relief_policy
            .map(crate::ResolvedPlanetReliefPolicy::policy)
            .or_else(|| {
                definition
                    .filter(|definition| definition.mode == 1)
                    .map(|_| crate::PlanetReliefSamplingPolicy::identity())
            })
            .or_else(|| {
                definition
                    .is_none()
                    .then(crate::PlanetReliefSamplingPolicy::identity)
            })
            .ok_or_else(|| {
                GenerationError::invalid_manifest(
                    "procedural planet requires a resolved relief sampling policy",
                )
            })?;
        Self::new_with_relief_policy(generation, surface, definition, inputs, policy)
    }

    /// Constructs terrain with a scenario-selected recipe before its map exists.
    pub fn new_with_relief_policy(
        generation: GenerationSpec,
        surface: PlanetSurfaceSpec,
        definition: Option<PlanetDefinition>,
        inputs: crate::TerrainInputs,
        relief_policy: crate::PlanetReliefSamplingPolicy,
    ) -> Result<Self, GenerationError> {
        if inputs
            .resolved_relief_policy
            .is_some_and(|resolved| resolved.policy() != relief_policy)
        {
            return Err(GenerationError::invalid_manifest(
                "terrain relief policy conflicts with resolved content",
            ));
        }
        if u64::from(generation.position_quantum_mm()) > surface.radius_mm() / 1024 {
            return Err(GenerationError::invalid_manifest(
                "planet generation quantum does not fit the surface",
            ));
        }
        if let Some(definition) = definition {
            if !definition.is_valid()
                || definition.radius_mm != surface.radius_mm()
                || u64::from(definition.mountain_limit_mm) != surface.relief_mm()
            {
                return Err(GenerationError::invalid_manifest(
                    "planet definition does not match its surface",
                ));
            }
            if definition.mode == 1 {
                let geography = inputs.geography.as_ref().ok_or_else(|| {
                    GenerationError::sampling(crate::EarthSampleError::PackageUnavailable)
                })?;
                if geography.package_digest() != definition.package_digest {
                    return Err(GenerationError::sampling(
                        crate::EarthSampleError::PackageMismatch,
                    ));
                }
            }
        }
        Ok(Self {
            generation,
            surface,
            definition,
            inputs,
            material_manifest: None,
            surface_recipe: Default::default(),
            relief_policy,
        })
    }

    /// Constructs terrain for material and volume queries bound to the exact
    /// manifest selected by content resolution.
    pub fn for_manifest(
        manifest: minimium_world::world::WorldManifest,
        inputs: crate::TerrainInputs,
    ) -> Result<Self, GenerationError> {
        if inputs
            .material_profile
            .is_some_and(|profile| !profile.matches_manifest(manifest))
        {
            return Err(GenerationError::invalid_manifest(
                "terrain material profile does not match the world manifest",
            ));
        }
        let relief_policy = match inputs.resolved_relief_policy {
            Some(policy) if !policy.matches_manifest(manifest) => {
                return Err(GenerationError::invalid_manifest(
                    "terrain relief policy does not match the world manifest",
                ));
            }
            Some(policy) => policy.policy(),
            None if manifest.source() == minimium_world::world::WorldSource::Generated
                && manifest.topology() == minimium_world::world::WorldTopology::Planet =>
            {
                return Err(GenerationError::invalid_manifest(
                    "generated planet has no resolved relief sampling policy",
                ));
            }
            None => crate::PlanetReliefSamplingPolicy::identity(),
        };
        let mut terrain = Self::new_with_relief_policy(
            manifest.generation(),
            crate::planet_surface_for(manifest),
            manifest.planet(),
            inputs,
            relief_policy,
        )?;
        terrain.material_manifest = Some(manifest);
        Ok(terrain)
    }

    pub fn with_surface_recipe(
        mut self,
        recipe: crate::surface_habitat::SurfaceRecipe,
    ) -> Option<Self> {
        if !recipe.valid() {
            return None;
        }
        self.surface_recipe = recipe;
        Some(self)
    }

    pub fn inputs(&self) -> &crate::TerrainInputs {
        &self.inputs
    }

    pub const fn radius_mm(&self) -> u64 {
        self.surface.radius_mm()
    }

    pub fn sea_level_mm(&self) -> i64 {
        self.definition
            .map_or(0, |definition| definition.sea_level_mm)
    }

    pub fn is_earth(&self) -> bool {
        self.definition
            .is_some_and(|definition| definition.mode == 1)
    }

    pub fn terrain_edits(&self) -> Option<&minimium_terrain::TerrainSnapshot> {
        self.inputs.edits.as_deref()
    }

    pub(crate) fn land_cover(&self, direction: [f64; 3]) -> Option<[f32; 6]> {
        if self.is_earth() {
            self.inputs.geography.as_ref()?.land_cover(direction)
        } else {
            None
        }
    }

    /// Presentation-only conversion from scaled terrain metres to source metres.
    pub fn presentation_height_scale(&self) -> f32 {
        self.definition.map_or(1.0, |definition| {
            if definition.mode == 1 {
                definition.height_scale_permille.max(1) as f32 / 1000.0
            } else {
                1.0
            }
        })
    }

    pub fn sample(&self, cube_sample: CubeSampleKey) -> Result<PlanetPoint, GenerationError> {
        self.sample_surface(cube_sample, false)
    }

    /// Ground retains Earth bathymetry beneath the separate ocean for collision
    /// and presentation alike.
    pub fn ground_sample(
        &self,
        cube_sample: CubeSampleKey,
    ) -> Result<PlanetPoint, GenerationError> {
        self.sample_surface(cube_sample, false)
    }

    /// Canonical sea-level vertex, with the same quantization as planet ground.
    pub fn sea_position(&self, cube_sample: CubeSampleKey) -> Position3Mm {
        let radius = (self.radius_mm() as i64 + self.sea_level_mm()).max(1) as u64;
        self.position_at(cube_sample, radius)
    }

    fn sample_surface(
        &self,
        cube_sample: CubeSampleKey,
        sea_clamped: bool,
    ) -> Result<PlanetPoint, GenerationError> {
        let (offset, ocean) = if let Some(definition) =
            self.definition.filter(|definition| definition.mode == 1)
        {
            let direction = cube_sample.coordinates().map(f64::from);
            let elevation_m = self
                .inputs
                .geography
                .as_ref()
                .map(|geography| geography.elevation_m(direction))
                .unwrap_or(Err(crate::EarthSampleError::PackageUnavailable))
                .map_err(GenerationError::sampling)?;
            if !elevation_m.is_finite() {
                return Err(GenerationError::sampling(
                    crate::EarthSampleError::InvalidElevation,
                ));
            }
            let elevation = definition.earth_elevation_mm(elevation_m).round() as i64;
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
            let ground = (elevation + detail).min(i64::from(definition.mountain_limit_mm));
            let offset = if sea_clamped {
                (elevation + detail)
                    .max(definition.sea_level_mm)
                    .min(i64::from(definition.mountain_limit_mm))
            } else {
                ground.max(1 - self.radius_mm() as i64)
            };
            (offset, ocean)
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
            let clearance = self.relief_policy.attenuation_permille(cube_sample);
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
        let displacement = self.terrain_edits().map_or(0.0, |edits| {
            edits.sample(cube_sample.coordinates().map(f64::from)).0
        });
        let height_mm = (self.surface.radius_mm as i64
            + offset
            + (displacement * 1000.0).round() as i64)
            .max(1) as u64;
        let position_mm = self.position_at(cube_sample, height_mm);
        Ok(PlanetPoint {
            cube_sample,
            height_mm,
            position_mm,
            ocean,
        })
    }

    pub fn earth_elevation_upper_bound_m(
        &self,
        direction: [f64; 3],
        angle: f64,
    ) -> Result<f64, GenerationError> {
        let bound = self
            .inputs
            .geography
            .as_ref()
            .map_or(
                Err(crate::EarthSampleError::PackageUnavailable),
                |geography| geography.elevation_upper_bound_m(direction, angle),
            )
            .map_err(GenerationError::sampling)?;
        if !bound.is_finite() {
            return Err(GenerationError::sampling(
                crate::EarthSampleError::InvalidElevation,
            ));
        }
        Ok(bound)
    }

    /// Unexaggerated source metres above the configured sea datum for climate.
    pub fn source_elevation_m(&self, direction: [f64; 3]) -> Result<Option<f64>, GenerationError> {
        let Some(definition) = self.definition.filter(|p| p.mode == 1) else {
            return Ok(None);
        };
        let raw = self
            .inputs
            .geography
            .as_ref()
            .ok_or_else(|| GenerationError::sampling(crate::EarthSampleError::PackageUnavailable))?
            .elevation_m(direction)
            .map_err(GenerationError::sampling)?;
        if !raw.is_finite() {
            return Err(GenerationError::sampling(
                crate::EarthSampleError::InvalidElevation,
            ));
        }
        Ok(Some(
            raw - definition.sea_level_mm as f64 / f64::from(definition.height_scale_permille),
        ))
    }

    pub fn has_peak_relief(&self) -> bool {
        self.definition.is_some_and(|p| p.peak_relief.enabled())
    }

    fn position_at(&self, sample: CubeSampleKey, radius_mm: u64) -> Position3Mm {
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
