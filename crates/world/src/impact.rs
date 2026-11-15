//! Bounded body/impact values shared by simulation, replication and persistence.
use crate::terrain_edits::{length, CraterStamp};
use crate::world::WorldInstanceId;

pub const MAX_ACTIVE_BODIES: usize = 8;
pub const BODY_RECORD_BYTES: usize = 162;
pub const BODY_COMPONENT: crate::entity::ComponentTypeId =
    match crate::entity::ComponentTypeId::new(0x100) {
        Some(id) => id,
        None => unreachable!(),
    };
pub const BODY_SCHEMA: crate::entity::SchemaVersion = match crate::entity::SchemaVersion::new(1) {
    Some(id) => id,
    None => unreachable!(),
};

pub fn register_component(
    registry: &mut crate::entity::ComponentRegistry,
) -> Result<(), crate::entity::ComponentError> {
    registry.register(component_descriptor())
}

fn component_descriptor() -> crate::entity::ComponentDescriptor {
    crate::entity::ComponentDescriptor {
        id: BODY_COMPONENT,
        name: "engine.impact_body",
        schema: BODY_SCHEMA,
        validate_payload: |bytes| {
            BodyState::decode(bytes)
                .map(|_| ())
                .ok_or(crate::entity::ComponentError::InvalidPayload)
        },
        replication: crate::entity::ReplicationPolicy::ServerOnly,
        visibility: crate::entity::ReplicationVisibility::Spatial {
            position: |bytes| Some(BodyState::decode(bytes)?.position.map(|v| v as f32)),
        },
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImpactProfile {
    pub chance_per_10000: u16,
    pub initial_mass_kg: f64,
    pub density_kg_m3: f64,
    pub crater_radius_m: f64,
}

impl ImpactProfile {
    pub fn valid(self) -> bool {
        self.chance_per_10000 <= 10000
            && self.initial_mass_kg.is_finite()
            && (1000.0..=1e7).contains(&self.initial_mass_kg)
            && self.density_kg_m3.is_finite()
            && (1000.0..=10000.0).contains(&self.density_kg_m3)
            && (6.0..=80.0).contains(&self.crater_radius_m)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyState {
    pub world_seed: u128,
    pub id: u64,
    pub world: WorldInstanceId,
    pub seed: u64,
    /// 0 flight, 1 landed, 2 collected. Terrain survives collection.
    pub phase: u8,
    pub position: [f64; 3],
    /// SI m/s; position is in game-world metres using metres_scale.
    pub velocity: [f64; 3],
    pub mass_kg: f64,
    pub initial_mass_kg: f64,
    pub density_kg_m3: f64,
    pub metres_scale: f64,
    pub planet_radius_m: f64,
    pub age_s: f64,
    pub crater_radius_m: f64,
    pub energy_j: f64,
    pub fragmentation: u8,
}

impl BodyState {
    /// Durable identity and completed terrain cannot change between checkpoints.
    pub fn follows(&self, old: &Self) -> bool {
        self.valid()
            && self.id == old.id
            && self.world == old.world
            && self.world_seed == old.world_seed
            && self.seed == old.seed
            && self.planet_radius_m == old.planet_radius_m
            && self.metres_scale == old.metres_scale
            && self.density_kg_m3 == old.density_kg_m3
            && self.initial_mass_kg == old.initial_mass_kg
            && self.phase >= old.phase
            && self.age_s >= old.age_s
            && self.fragmentation >= old.fragmentation
            && self.mass_kg <= old.mass_kg
            && (old.phase == 0
                || (self.position == old.position
                    && self.crater_radius_m == old.crater_radius_m
                    && self.energy_j == old.energy_j))
    }

    pub fn valid(&self) -> bool {
        self.id != 0
            && self.phase <= 2
            && (self.phase == 0 || self.velocity == [0.0; 3])
            && self.fragmentation <= 3
            && self.position.iter().all(|x| x.is_finite() && x.abs() < 1e8)
            && length(self.position) > 1.0
            && self
                .velocity
                .iter()
                .all(|x| x.is_finite() && x.abs() <= 80000.0)
            && [
                self.mass_kg,
                self.initial_mass_kg,
                self.density_kg_m3,
                self.metres_scale,
                self.planet_radius_m,
                self.age_s,
                self.crater_radius_m,
                self.energy_j,
            ]
            .iter()
            .all(|v| v.is_finite())
            && self.mass_kg > 0.0
            && self.mass_kg <= self.initial_mass_kg
            && self.initial_mass_kg <= 1e7
            && (1000.0..=10000.0).contains(&self.density_kg_m3)
            && (0.001..=10.0).contains(&self.metres_scale)
            && (5.0..=6_371_000.0).contains(&self.planet_radius_m)
            && (0.0..=3600.0).contains(&self.age_s)
            && (6.0..=80.0).contains(&self.crater_radius_m)
            && (0.0..=1e18).contains(&self.energy_j)
    }

    pub fn radius_m(&self) -> f64 {
        (self.mass_kg / self.density_kg_m3 * 3.0 / (4.0 * std::f64::consts::PI)).cbrt()
    }

    pub fn crater(&self) -> Option<CraterStamp> {
        (self.phase != 0).then(|| CraterStamp {
            id: self.id,
            center: self.position,
            radius_m: self.crater_radius_m,
            depth_m: self.crater_radius_m * 0.18,
            seed: self.seed,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(BODY_RECORD_BYTES);
        out.extend(self.id.to_le_bytes());
        out.extend(self.world.get().to_le_bytes());
        out.extend(self.seed.to_le_bytes());
        out.push(self.phase);
        out.push(self.fragmentation);
        for value in self
            .position
            .iter()
            .chain(self.velocity.iter())
            .copied()
            .chain([
                self.mass_kg,
                self.initial_mass_kg,
                self.density_kg_m3,
                self.metres_scale,
                self.planet_radius_m,
                self.age_s,
                self.crater_radius_m,
                self.energy_j,
            ])
        {
            out.extend(value.to_le_bytes());
        }
        out.extend(self.world_seed.to_le_bytes());
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != BODY_RECORD_BYTES {
            return None;
        }
        let read = |at| Some(f64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?));
        let body = Self {
            world_seed: u128::from_le_bytes(bytes[146..162].try_into().ok()?),
            id: u64::from_le_bytes(bytes[0..8].try_into().ok()?),
            world: WorldInstanceId::new(u128::from_le_bytes(bytes[8..24].try_into().ok()?))?,
            seed: u64::from_le_bytes(bytes[24..32].try_into().ok()?),
            phase: bytes[32],
            fragmentation: bytes[33],
            position: [read(34)?, read(42)?, read(50)?],
            velocity: [read(58)?, read(66)?, read(74)?],
            mass_kg: read(82)?,
            initial_mass_kg: read(90)?,
            density_kg_m3: read(98)?,
            metres_scale: read(106)?,
            planet_radius_m: read(114)?,
            age_s: read(122)?,
            crater_radius_m: read(130)?,
            energy_j: read(138)?,
        };
        body.valid().then_some(body)
    }
}
