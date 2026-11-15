use crate::field::{BaseTerrain, ChunkImage, TerrainStore, SAMPLES_PER_CHUNK};
use minimium_world::{
    terrain::{
        valid_position, TerrainChunkKey, TerrainMaterialSet, TerrainRevision, TERRAIN_CELL_M,
        TERRAIN_CHUNK_M,
    },
    terrain_edits::{length, CraterStamp},
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug)]
pub struct TerrainEditRequest {
    pub mutation_id: u64,
    pub center_m: [f64; 3],
    pub deformation: TerrainDeformation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainDeformationError {
    InvalidRadius,
    InvalidMaterials,
}

/// Validated generic radial terrain deformation. Tool and material policy
/// selection belongs to the authoritative gameplay owner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainDeformation {
    radius_m: f64,
    materials: TerrainMaterialSet,
}

impl TerrainDeformation {
    pub fn new(
        radius_m: f64,
        materials: TerrainMaterialSet,
    ) -> Result<Self, TerrainDeformationError> {
        if !radius_m.is_finite() || radius_m < 0.25 {
            return Err(TerrainDeformationError::InvalidRadius);
        }
        if !materials.valid() {
            return Err(TerrainDeformationError::InvalidMaterials);
        }
        Ok(Self {
            radius_m,
            materials,
        })
    }

    pub const fn radius_m(self) -> f64 {
        self.radius_m
    }

    pub const fn materials(self) -> TerrainMaterialSet {
        self.materials
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerrainEditError {
    Invalid,
    Unavailable,
    Capacity,
    Stale,
    NoMaterial,
    RevisionExhausted,
}

/// Result data for durable storage; never replay a future version of the brush.
#[derive(Clone, Debug)]
pub struct PreparedTerrainEdit {
    pub(crate) mutation_id: u64,
    pub(crate) expected: Vec<(TerrainChunkKey, TerrainRevision)>,
    pub(crate) chunks: Vec<Arc<ChunkImage>>,
}

impl PreparedTerrainEdit {
    /// Reconstruct a stored result without invoking brush generation again.
    pub fn from_chunks(
        mutation_id: u64,
        expected: Vec<(TerrainChunkKey, TerrainRevision)>,
        chunks: Vec<Arc<ChunkImage>>,
    ) -> Result<Self, TerrainEditError> {
        if mutation_id == 0
            || chunks.is_empty()
            || chunks.len() > 27
            || chunks.len() != expected.len()
        {
            return Err(TerrainEditError::Invalid);
        }
        let mut keys = std::collections::BTreeSet::new();
        for ((key, revision), chunk) in expected.iter().zip(&chunks) {
            if *key != chunk.key || !keys.insert(*key) || revision.next() != Some(chunk.revision) {
                return Err(TerrainEditError::Invalid);
            }
        }
        Ok(Self {
            mutation_id,
            expected,
            chunks,
        })
    }

    pub fn mutation_id(&self) -> u64 {
        self.mutation_id
    }

    pub fn expected(&self) -> &[(TerrainChunkKey, TerrainRevision)] {
        &self.expected
    }

    pub fn chunks(&self) -> &[Arc<ChunkImage>] {
        &self.chunks
    }
}

impl TerrainStore {
    pub fn prepare(
        &self,
        base: &dyn BaseTerrain,
        request: TerrainEditRequest,
    ) -> Result<PreparedTerrainEdit, TerrainEditError> {
        if !valid_position(request.center_m)
            || !(0.25..=self.config.max_edit_radius_m).contains(&request.deformation.radius_m())
        {
            return Err(TerrainEditError::Invalid);
        }
        if request.mutation_id <= self.last_mutation {
            return Err(TerrainEditError::Stale);
        }
        let lo = TerrainChunkKey::at(
            self.world,
            request
                .center_m
                .map(|v| v - request.deformation.radius_m() - TERRAIN_CELL_M),
        )
        .ok_or(TerrainEditError::Invalid)?
        .coordinates;
        let hi = TerrainChunkKey::at(
            self.world,
            request
                .center_m
                .map(|v| v + request.deformation.radius_m() + TERRAIN_CELL_M),
        )
        .ok_or(TerrainEditError::Invalid)?
        .coordinates;
        let mut prepared = PreparedTerrainEdit {
            mutation_id: request.mutation_id,
            expected: Vec::new(),
            chunks: Vec::new(),
        };
        let mut modified = false;
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    modified |= self.prepare_chunk(base, request, [x, y, z], &mut prepared)?;
                }
            }
        }
        if !modified {
            return Err(TerrainEditError::NoMaterial);
        }
        self.validate(&prepared)?;
        Ok(prepared)
    }

    /// Reprojects active voxel chunks onto a committed crater field. Sparse
    /// chunks outside the crater's angular support remain owned by the base port.
    pub fn prepare_crater(
        &self,
        old_base: &dyn BaseTerrain,
        new_base: &dyn BaseTerrain,
        stamp: CraterStamp,
        mutation_id: u64,
    ) -> Result<PreparedTerrainEdit, TerrainEditError> {
        if !stamp.valid() || mutation_id == 0 {
            return Err(TerrainEditError::Invalid);
        }
        if mutation_id <= self.last_mutation {
            return Err(TerrainEditError::Stale);
        }
        let center_length = length(stamp.center);
        let center_direction = stamp.center.map(|v| v / center_length);
        let planet_radius = center_length;
        let support_m = stamp.radius_m * 2.0 + TERRAIN_CHUNK_M * 1.75;
        let mut keys = Vec::new();
        for key in self.chunks.keys().copied() {
            let center = key.origin_m().map(|v| v + TERRAIN_CHUNK_M * 0.5);
            if old_base.sample(center).is_none() {
                continue;
            }
            let radius = length(center);
            if radius < 1.0 {
                continue;
            }
            let direction = center.map(|v| v / radius);
            let cosine = direction
                .iter()
                .zip(center_direction)
                .map(|(a, b)| a * b)
                .sum::<f64>()
                .clamp(-1.0, 1.0);
            let surface_distance = cosine.acos() * planet_radius;
            if surface_distance <= support_m {
                keys.push(key);
            }
        }
        if keys.is_empty() {
            return Err(TerrainEditError::NoMaterial);
        }
        if keys.len() > 27 || self.chunks.len() > self.config.max_chunks {
            return Err(TerrainEditError::Capacity);
        }
        let mut prepared = PreparedTerrainEdit {
            mutation_id,
            expected: Vec::with_capacity(keys.len()),
            chunks: Vec::with_capacity(keys.len()),
        };
        let mut changed = false;
        for key in keys {
            changed |= self.prepare_crater_chunk(key, old_base, new_base, &mut prepared)?;
        }
        if !changed {
            return Err(TerrainEditError::NoMaterial);
        }
        self.validate(&prepared)?;
        Ok(prepared)
    }

    fn prepare_crater_chunk(
        &self,
        key: TerrainChunkKey,
        old_base: &dyn BaseTerrain,
        new_base: &dyn BaseTerrain,
        prepared: &mut PreparedTerrainEdit,
    ) -> Result<bool, TerrainEditError> {
        let old = self.chunks.get(&key).ok_or(TerrainEditError::Stale)?;
        let expected = old.revision;
        let revision = expected.next().ok_or(TerrainEditError::RevisionExhausted)?;
        let mut samples = Vec::with_capacity(SAMPLES_PER_CHUNK);
        let mut changed = false;
        for index in 0..SAMPLES_PER_CHUNK {
            let local = [index % 32, (index / 32) % 32, index / 1024];
            let position = std::array::from_fn(|i| {
                (f64::from(key.coordinates[i]) * 32.0 + local[i] as f64) * TERRAIN_CELL_M
            });
            let old_ground = old_base
                .sample(position)
                .ok_or(TerrainEditError::Unavailable)?;
            let new_ground = new_base
                .sample(position)
                .ok_or(TerrainEditError::Unavailable)?;
            let mut sample = old.samples[index];
            let bowl = sample.distance_m.max(new_ground.distance_m);
            let rim_shell = (-old_ground.distance_m).max(new_ground.distance_m);
            let distance = bowl.min(rim_shell);
            if !distance.is_finite() {
                return Err(TerrainEditError::Invalid);
            }
            if (distance - sample.distance_m).abs() > 1.0e-6 {
                sample.distance_m = distance;
                if distance < 0.0 {
                    sample.material = new_ground.material;
                }
                changed = true;
            }
            sample.revision = revision;
            samples.push(sample);
        }
        prepared.expected.push((key, expected));
        prepared.chunks.push(Arc::new(ChunkImage {
            key,
            revision,
            samples,
        }));
        Ok(changed)
    }

    fn prepare_chunk(
        &self,
        base: &dyn BaseTerrain,
        request: TerrainEditRequest,
        coordinates: [i32; 3],
        prepared: &mut PreparedTerrainEdit,
    ) -> Result<bool, TerrainEditError> {
        let key = TerrainChunkKey {
            world: self.world,
            coordinates,
        };
        if !key.valid() {
            return Err(TerrainEditError::Invalid);
        }
        let old = self.chunks.get(&key);
        let expected = old.map_or(TerrainRevision(0), |chunk| chunk.revision);
        let revision = expected.next().ok_or(TerrainEditError::RevisionExhausted)?;
        let mut samples = Vec::with_capacity(SAMPLES_PER_CHUNK);
        let mut changed = false;
        for index in 0..SAMPLES_PER_CHUNK {
            let local = [index % 32, (index / 32) % 32, index / 1024];
            let position = std::array::from_fn(|i| {
                (f64::from(coordinates[i]) * 32.0 + local[i] as f64) * TERRAIN_CELL_M
            });
            let mut sample = if let Some(chunk) = old {
                chunk.samples[index]
            } else {
                base.sample(position).ok_or(TerrainEditError::Unavailable)?
            };
            if !sample.distance_m.is_finite() {
                return Err(TerrainEditError::Invalid);
            }
            let distance = (0..3)
                .map(|i| (position[i] - request.center_m[i]).powi(2))
                .sum::<f64>()
                .sqrt();
            let cut = request.deformation.radius_m() - distance;
            // Keep the material label behind the boundary for surface shading.
            if distance <= request.deformation.radius_m() + TERRAIN_CELL_M
                && request.deformation.materials().contains(sample.material)
                && cut > sample.distance_m
            {
                sample.distance_m = cut;
                changed = true;
            }
            sample.revision = revision;
            samples.push(sample);
        }
        prepared.expected.push((key, expected));
        prepared.chunks.push(Arc::new(ChunkImage {
            key,
            revision,
            samples,
        }));
        Ok(changed)
    }

    pub fn validate(&self, edit: &PreparedTerrainEdit) -> Result<(), TerrainEditError> {
        if edit.mutation_id <= self.last_mutation {
            return Err(TerrainEditError::Stale);
        }
        let mut additional = 0;
        for &(key, expected) in &edit.expected {
            if key.world != self.world
                || self
                    .chunks
                    .get(&key)
                    .map_or(TerrainRevision(0), |c| c.revision)
                    != expected
            {
                return Err(TerrainEditError::Stale);
            }
            additional += usize::from(!self.chunks.contains_key(&key));
        }
        if self.chunks.len() + additional > self.config.max_chunks {
            return Err(TerrainEditError::Capacity);
        }
        Ok(())
    }

    /// Host must hold reservations and a durable acknowledgement before calling.
    pub fn commit(&mut self, edit: PreparedTerrainEdit) -> Result<(), TerrainEditError> {
        self.validate(&edit)?;
        for chunk in edit.chunks {
            self.chunks.insert(chunk.key, chunk);
        }
        self.last_mutation = edit.mutation_id;
        Ok(())
    }
}
