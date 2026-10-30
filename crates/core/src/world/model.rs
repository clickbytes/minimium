//! Stable, platform-independent identities and world-generation contracts.

use std::fmt;

macro_rules! nonzero_u128_id {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u128);

        impl $name {
            pub const fn new(value: u128) -> Option<Self> {
                if value == 0 {
                    None
                } else {
                    Some(Self(value))
                }
            }

            pub const fn get(self) -> u128 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

macro_rules! nonzero_u64_revision {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u64);

        impl $name {
            pub const fn new(value: u64) -> Option<Self> {
                if value == 0 {
                    None
                } else {
                    Some(Self(value))
                }
            }

            pub const fn get(self) -> u64 {
                self.0
            }
        }
    };
}

nonzero_u128_id!(WorldDefinitionId);
nonzero_u128_id!(WorldInstanceId);
nonzero_u128_id!(ObjectOriginId);
nonzero_u128_id!(TransitionId);
nonzero_u64_revision!(WorldContentRevision);
nonzero_u64_revision!(SectorRevision);
nonzero_u64_revision!(MembershipEpoch);

pub const MAX_SECTOR_OBJECT_STATES: usize = 128;
pub const MAX_OBJECT_STATE_BYTES: usize = 256;
pub const MAX_SECTOR_OBJECT_STATE_BYTES: usize = 16 * 1024;
pub const MAX_PERSISTED_WORLD_SECTORS: usize = 4096;
pub const MAX_PERSISTED_WORLD_OBJECT_STATES: usize = 16_384;
pub const MAX_PERSISTED_WORLD_OBJECT_STATE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WorldSeed(u128);

impl WorldSeed {
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u128 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GenerationRevision(u32);

impl GenerationRevision {
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Generator settings that affect logical output and therefore belong in a manifest.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GenerationSpec {
    seed: WorldSeed,
    revision: GenerationRevision,
    position_quantum_mm: u32,
}

impl GenerationSpec {
    pub const fn new(
        seed: WorldSeed,
        revision: GenerationRevision,
        position_quantum_mm: u32,
    ) -> Option<Self> {
        if position_quantum_mm == 0 {
            None
        } else {
            Some(Self {
                seed,
                revision,
                position_quantum_mm,
            })
        }
    }

    pub const fn seed(self) -> WorldSeed {
        self.seed
    }

    pub const fn revision(self) -> GenerationRevision {
        self.revision
    }

    /// Resolution of generated positions in millimeters; zero is never valid.
    pub const fn position_quantum_mm(self) -> u32 {
        self.position_quantum_mm
    }
}

/// Opaque digest of the generator and its logical content contract.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GeneratorDigest([u8; 32]);

impl GeneratorDigest {
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn legacy_map_fingerprint(fingerprint: u64) -> Self {
        let mut bytes = [0_u8; 32];
        for (index, chunk) in bytes.as_chunks_mut::<8>().0.iter_mut().enumerate() {
            let value = fingerprint.rotate_left((index * 13) as u32) ^ index as u64;
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        Self::new(bytes)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum WorldTopology {
    Planet,
    Dungeon,
}

/// Parameters that change the generated planet. Present from generator v3.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlanetDefinition {
    pub mode: u8,
    pub radius_mm: u64,
    pub height_scale_permille: u32,
    pub sea_level_mm: i64,
    pub mountain_limit_mm: u32,
    pub detail_strength_mm: u32,
    pub detail_wavelength_mm: u32,
    pub mountain_band_frequency: u32,
    pub package_digest: [u8; 32],
}

impl PlanetDefinition {
    pub const fn is_valid(self) -> bool {
        self.mode <= 1
            && self.radius_mm >= 318_550_000
            && self.radius_mm <= 6_371_000_000
            && self.height_scale_permille >= 1
            && self.height_scale_permille <= 1000
            && self.mountain_limit_mm <= 5_000_000
            && self.sea_level_mm <= self.mountain_limit_mm as i64
            && self.detail_strength_mm <= 1_000_000
            && self.detail_wavelength_mm > 0
            && self.detail_wavelength_mm <= 1_000_000_000
            && self.mountain_band_frequency > 0
            && self.mountain_band_frequency <= 1024
            && self.sea_level_mm >= -20_000_000
            && self.sea_level_mm <= 20_000_000
    }
}

/// A published immutable description used to create one or more instances.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldManifest {
    definition_id: WorldDefinitionId,
    topology: WorldTopology,
    generation: GenerationSpec,
    content_revision: WorldContentRevision,
    generator_digest: GeneratorDigest,
    planet: Option<PlanetDefinition>,
}

impl WorldManifest {
    pub const fn new(
        definition_id: WorldDefinitionId,
        topology: WorldTopology,
        generation: GenerationSpec,
        content_revision: WorldContentRevision,
        generator_digest: GeneratorDigest,
    ) -> Self {
        Self {
            definition_id,
            topology,
            generation,
            content_revision,
            generator_digest,
            planet: None,
        }
    }

    pub const fn definition_id(self) -> WorldDefinitionId {
        self.definition_id
    }

    pub const fn topology(self) -> WorldTopology {
        self.topology
    }

    pub const fn generation(self) -> GenerationSpec {
        self.generation
    }

    pub const fn content_revision(self) -> WorldContentRevision {
        self.content_revision
    }

    pub const fn generator_digest(self) -> GeneratorDigest {
        self.generator_digest
    }

    pub const fn with_planet(mut self, planet: PlanetDefinition) -> Self {
        self.planet = Some(planet);
        self
    }

    pub const fn planet(self) -> Option<PlanetDefinition> {
        self.planet
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Position3Mm(pub [i64; 3]);

impl Position3Mm {
    pub const fn new(x: i64, y: i64, z: i64) -> Self {
        Self([x, y, z])
    }

    pub const fn coordinates(self) -> [i64; 3] {
        self.0
    }
}

/// Canonical unit quaternion with signed Q1.30 components.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldOrientationQ30([i32; 4]);

impl WorldOrientationQ30 {
    const ONE: i128 = 1_i128 << 30;
    const NORM_TOLERANCE: i128 = (1_i128 << 60) / 1024;

    pub const IDENTITY: Self = Self([1 << 30, 0, 0, 0]);

    /// Accepts a near-unit quaternion, then canonicalizes q and -q to one value.
    pub fn new(mut components: [i32; 4]) -> Option<Self> {
        let norm_squared = components
            .iter()
            .map(|value| i128::from(*value) * i128::from(*value))
            .sum::<i128>();
        if (norm_squared - Self::ONE * Self::ONE).abs() > Self::NORM_TOLERANCE {
            return None;
        }
        if components.iter().copied().find(|value| *value != 0)? < 0 {
            components = components.map(|value| -value);
        }
        Some(Self(components))
    }

    pub const fn components(self) -> [i32; 4] {
        self.0
    }
}

/// A player's complete authoritative location. Position is quantized to millimeters.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorldLocation {
    instance: WorldInstanceId,
    position: Position3Mm,
    orientation: WorldOrientationQ30,
}

impl WorldLocation {
    pub const fn new(
        instance: WorldInstanceId,
        position: Position3Mm,
        orientation: WorldOrientationQ30,
    ) -> Self {
        Self {
            instance,
            position,
            orientation,
        }
    }

    pub const fn instance(self) -> WorldInstanceId {
        self.instance
    }

    pub const fn position(self) -> Position3Mm {
        self.position
    }

    pub const fn orientation(self) -> WorldOrientationQ30 {
        self.orientation
    }
}

/// Opaque, versioned-by-owner state for a generated or authored object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldObjectState {
    origin: ObjectOriginId,
    data: Vec<u8>,
}

impl WorldObjectState {
    pub fn new(origin: ObjectOriginId, data: Vec<u8>) -> Option<Self> {
        (data.len() <= MAX_OBJECT_STATE_BYTES).then_some(Self { origin, data })
    }

    pub const fn origin(&self) -> ObjectOriginId {
        self.origin
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

/// Complete durable logical state for one world sector at a specific revision.
/// Omitted object origins are removed from that sector's materialized state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorldSectorState {
    key: super::SectorKey,
    revision: SectorRevision,
    objects: Vec<WorldObjectState>,
}

impl WorldSectorState {
    pub fn new(
        key: super::SectorKey,
        revision: SectorRevision,
        objects: Vec<WorldObjectState>,
    ) -> Option<Self> {
        use std::collections::HashSet;
        let mut origins = HashSet::with_capacity(objects.len());
        let state_bytes = objects
            .iter()
            .map(|object| object.data.len())
            .sum::<usize>();
        if objects.len() > MAX_SECTOR_OBJECT_STATES
            || state_bytes > MAX_SECTOR_OBJECT_STATE_BYTES
            || objects.iter().any(|object| !origins.insert(object.origin))
        {
            return None;
        }
        Some(Self {
            key,
            revision,
            objects,
        })
    }

    pub const fn key(&self) -> super::SectorKey {
        self.key
    }

    pub const fn revision(&self) -> SectorRevision {
        self.revision
    }

    pub fn objects(&self) -> &[WorldObjectState] {
        &self.objects
    }
}
