//! GPU sector-geometry buffers and their shadow bounds cache.
use crate::{geometry, geometry_rebase::RebaseLayout, shadow_geometry::ShadowBounds};
use minimium_procgen::SectorGeometry;
use minimium_world::world::{GenerationRevision, SectorKey, WorldContentRevision, WorldInstanceId};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};
use wgpu::util::DeviceExt;

mod legacy_cache;
mod lifecycle;
mod local_water;
mod prepared_upload;

type CatalogKey = (WorldInstanceId, SectorKey, u8);
type PreparedStageKey = (WorldInstanceId, GeometryStageId, SectorKey, u8);
type SceneStageKey = (WorldInstanceId, GeometryStageId);
const MAX_STAGED_SCENE_OWNERS: usize = 2;
const FRAME_UPLOAD_PATCH_BUDGET: u32 = 8;
const FRAME_UPLOAD_BYTE_BUDGET: u64 = 4 * 1024 * 1024;

/// Identifies one active or pending scene's upload staging area within a world.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct GeometryStageId {
    kind: u8,
    primary: u128,
    secondary: u128,
}

impl GeometryStageId {
    pub const fn new(kind: u8, primary: u128, secondary: u128) -> Self {
        Self {
            kind,
            primary,
            secondary,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GeometryPublicationStamp {
    pub instance: WorldInstanceId,
    pub generation: GenerationRevision,
    pub content_revision: WorldContentRevision,
    pub world_fingerprint: u64,
    pub geometry_revision: u64,
    pub origin_generation: u64,
}

#[derive(Clone, Debug)]
pub struct GeometryPublication {
    pub stamp: GeometryPublicationStamp,
    pub stage: GeometryStageId,
    pub keys: Vec<(SectorKey, u8)>,
}

/// Owns per-world sector vertex buffers and geometry bounds used by shadow selection.
#[derive(Default)]
pub struct WorldGeometryGpu {
    rebase: Option<crate::geometry_rebase::GeometryRebase>,
    budget: UploadBudget,
    terrain: HashMap<CatalogKey, TerrainBuffers>,
    catalog: HashMap<CatalogKey, (wgpu::Buffer, u32)>,
    catalog_origin_generation: HashMap<CatalogKey, u64>,
    versions: HashMap<WorldInstanceId, (GenerationRevision, WorldContentRevision)>,
    bounds: HashMap<CatalogKey, ShadowBounds>,
    sources: HashMap<CatalogKey, Arc<SectorGeometry>>,
    prepared_sources: HashMap<CatalogKey, Arc<crate::prepared_sector::PreparedSector>>,
    pending: HashMap<PreparedStageKey, prepared_upload::PendingGeometry>,
    pending_raw: HashMap<PreparedStageKey, prepared_upload::PendingRawGeometry>,
    staged_versions:
        HashMap<(WorldInstanceId, GeometryStageId), (GenerationRevision, WorldContentRevision)>,
    stage_order: VecDeque<SceneStageKey>,
    priority_cursor: Option<SceneStageKey>,
    priority_stage: Option<SceneStageKey>,
    priority_stage_serviced: bool,
    water: HashMap<WorldInstanceId, local_water::LocalWaterSlot>,
    released_publication_bytes: u64,
    publication: Option<GeometryPublication>,
    next_geometry_revision: u64,
    origin_generation: u64,
}

struct UploadBudget {
    patches: u32,
    bytes: u64,
}
impl Default for UploadBudget {
    fn default() -> Self {
        Self {
            patches: FRAME_UPLOAD_PATCH_BUDGET,
            bytes: FRAME_UPLOAD_BYTE_BUDGET,
        }
    }
}
pub struct TerrainBuffers {
    pub surface: wgpu::Buffer,
    pub surface_count: u32,
    pub ocean: wgpu::Buffer,
    pub ocean_count: u32,
    pub objects: wgpu::Buffer,
    pub object_count: u32,
}

impl TerrainBuffers {
    fn bytes(&self) -> u64 {
        self.surface.size() + self.ocean.size() + self.objects.size()
    }
}

/// Result of reconciling visible geometry against the current GPU cache.
pub struct GeometryCacheResult {
    pub ready: bool,
    pub allocated_bytes: u64,
    pub released_bytes: u64,
}

impl WorldGeometryGpu {
    pub(super) fn can_stage(&self, instance: WorldInstanceId, stage: GeometryStageId) -> bool {
        self.staged_versions.contains_key(&(instance, stage))
            || self.staged_versions.len() < MAX_STAGED_SCENE_OWNERS
    }

    pub(super) fn register_stage(&mut self, instance: WorldInstanceId, stage: GeometryStageId) {
        let key = (instance, stage);
        if !self.staged_versions.contains_key(&key) {
            self.stage_order.push_back(key);
        }
    }

    pub(super) fn stage_can_upload(
        &mut self,
        instance: WorldInstanceId,
        stage: GeometryStageId,
    ) -> bool {
        let key = (instance, stage);
        if self.priority_stage.is_none() {
            self.priority_stage = Some(key);
            self.priority_cursor = Some(key);
        }
        if self.priority_stage_serviced {
            self.priority_stage != Some(key)
        } else {
            self.priority_stage == Some(key)
        }
    }

    pub fn finish_scene_upload(&mut self, instance: WorldInstanceId, stage: GeometryStageId) {
        if self.priority_stage == Some((instance, stage)) {
            self.priority_stage_serviced = true;
        }
    }

    pub fn publication(&self) -> Option<&GeometryPublication> {
        self.publication.as_ref()
    }

    pub fn origin_generation(&self) -> u64 {
        self.origin_generation
    }

    pub fn invalidate_publication(&mut self) {
        self.publication = None;
    }

    pub fn publish_scene(
        &mut self,
        instance: WorldInstanceId,
        stage: GeometryStageId,
        fingerprint: u64,
        keys: &[(SectorKey, u8)],
    ) -> bool {
        let Some((generation, content_revision)) =
            self.staged_versions.get(&(instance, stage)).copied()
        else {
            return false;
        };
        let is_published_stage = self.publication.as_ref().is_some_and(|publication| {
            publication.stamp.instance == instance && publication.stage == stage
        });
        let complete = !keys.is_empty()
            && keys.iter().all(|(key, lod)| {
                let catalog_key = (instance, *key, *lod);
                self.pending
                    .get(&(instance, stage, *key, *lod))
                    .is_some_and(|pending| pending.origin_generation == self.origin_generation)
                    || self
                        .pending_raw
                        .get(&(instance, stage, *key, *lod))
                        .is_some_and(|pending| pending.origin_generation == self.origin_generation)
                    || (is_published_stage
                        && self.catalog.contains_key(&catalog_key)
                        && self.versions.get(&instance) == Some(&(generation, content_revision))
                        && self.catalog_origin_generation.get(&catalog_key)
                            == Some(&self.origin_generation))
            });
        if !complete {
            return false;
        }
        let geometry_changed =
            self.publish_prepared(instance, stage, keys) || self.publish_raw(instance, stage, keys);
        self.versions
            .insert(instance, (generation, content_revision));
        self.publish_local_water(instance, stage);
        if !geometry_changed
            && self.publication.as_ref().is_some_and(|publication| {
                publication.stamp.instance == instance
                    && publication.stage == stage
                    && publication.stamp.generation == generation
                    && publication.stamp.content_revision == content_revision
                    && publication.stamp.world_fingerprint == fingerprint
                    && publication.stamp.origin_generation == self.origin_generation
                    && publication.keys == keys
            })
        {
            return true;
        }
        let Some(revision) = self.next_geometry_revision.checked_add(1) else {
            self.publication = None;
            return false;
        };
        self.next_geometry_revision = revision;
        self.publication = Some(GeometryPublication {
            stamp: GeometryPublicationStamp {
                instance,
                generation,
                content_revision,
                world_fingerprint: fingerprint,
                geometry_revision: revision,
                origin_generation: self.origin_generation,
            },
            stage,
            keys: keys.to_vec(),
        });
        let released = self.release_scene_stage(instance, stage);
        self.released_publication_bytes = self.released_publication_bytes.saturating_add(released);
        true
    }

    /// Drops all unpublished geometry and water uploads for one scene owner.
    pub fn release_scene_stage(
        &mut self,
        instance: WorldInstanceId,
        stage: GeometryStageId,
    ) -> u64 {
        let mut released = 0;
        self.pending.retain(|(owner, candidate, _, _), image| {
            if *owner == instance && *candidate == stage {
                released += image.bytes();
                false
            } else {
                true
            }
        });
        self.pending_raw.retain(|(owner, candidate, _, _), image| {
            if *owner == instance && *candidate == stage {
                released += image.bytes();
                false
            } else {
                true
            }
        });
        if let Some(slot) = self.water.get_mut(&instance) {
            released += slot.release_stage(stage);
        }
        self.staged_versions.remove(&(instance, stage));
        self.stage_order.retain(|key| *key != (instance, stage));
        if self.priority_stage == Some((instance, stage)) {
            self.priority_stage = None;
            self.priority_stage_serviced = false;
        }
        released
    }

    pub fn begin_frame(&mut self) {
        self.budget = UploadBudget::default();
        self.stage_order
            .retain(|key| self.staged_versions.contains_key(key));
        self.priority_stage = if self.stage_order.is_empty() {
            None
        } else {
            let cursor = self.priority_cursor.and_then(|key| {
                self.stage_order
                    .iter()
                    .position(|candidate| *candidate == key)
            });
            let next = cursor.map_or(0, |index| (index + 1) % self.stage_order.len());
            self.stage_order.get(next).copied()
        };
        if let Some(stage) = self.priority_stage {
            self.priority_cursor = Some(stage);
        }
        self.priority_stage_serviced = false;
    }
    pub fn new(device: &wgpu::Device, compute: bool) -> Self {
        Self {
            rebase: compute.then(|| crate::geometry_rebase::GeometryRebase::new(device)),
            ..Self::default()
        }
    }
    pub fn rebase(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, delta: [f32; 3]) -> bool {
        let Some(origin_generation) = self.origin_generation.checked_add(1) else {
            self.publication = None;
            return false;
        };
        let Some(rebase) = &self.rebase else {
            self.origin_generation = origin_generation;
            self.publication = None;
            return false;
        };
        let flat = self
            .catalog
            .values()
            .map(|(b, _)| (b, RebaseLayout::Object));
        let terrain = self.terrain.values().flat_map(|t| {
            [
                (&t.surface, RebaseLayout::Terrain),
                (&t.ocean, RebaseLayout::Water),
                (&t.objects, RebaseLayout::Object),
            ]
        });
        let pending = self.pending.values().flat_map(|p| p.buffers());
        let pending_raw = self
            .pending_raw
            .values()
            .map(|pending| (&pending.buffer, RebaseLayout::Object));
        let water = self
            .water
            .values()
            .flat_map(|slot| slot.buffers().map(|buffer| (buffer, RebaseLayout::Water)));
        rebase.apply(
            device,
            queue,
            delta,
            flat.chain(terrain)
                .chain(pending)
                .chain(pending_raw)
                .chain(water),
        );
        self.origin_generation = origin_generation;
        self.catalog_origin_generation
            .values_mut()
            .for_each(|generation| *generation = origin_generation);
        for pending in self.pending.values_mut() {
            pending.origin_generation = origin_generation;
        }
        for pending in self.pending_raw.values_mut() {
            pending.origin_generation = origin_generation;
        }
        for slot in self.water.values_mut() {
            slot.rebase(origin_generation);
        }
        if let Some(mut publication) = self.publication.take() {
            let Some(revision) = self.next_geometry_revision.checked_add(1) else {
                return false;
            };
            self.next_geometry_revision = revision;
            publication.stamp.geometry_revision = revision;
            publication.stamp.origin_generation = origin_generation;
            self.publication = Some(publication);
        }
        true
    }
    fn usage(&self) -> wgpu::BufferUsages {
        wgpu::BufferUsages::VERTEX
            | if self.rebase.is_some() {
                wgpu::BufferUsages::STORAGE
            } else {
                wgpu::BufferUsages::empty()
            }
    }

    /// Conservative distance to resident terrain bounds, used only to improve
    /// depth precision when flying well clear of all visible geometry.
    pub fn surface_near(
        &self,
        instance: WorldInstanceId,
        active: &[(SectorKey, u8)],
        eye: [f64; 3],
    ) -> f32 {
        let distance = active
            .iter()
            .filter_map(|(k, l)| self.bounds.get(&(instance, *k, *l)))
            .map(|b| {
                (0..3)
                    .map(|a| {
                        (b.minimum[a] - eye[a])
                            .max(eye[a] - b.maximum[a])
                            .max(0.0)
                            .powi(2)
                    })
                    .sum::<f64>()
                    .sqrt()
            })
            .fold(f64::INFINITY, f64::min);
        if distance.is_finite() {
            // Keep precision proportional to actual clearance in orbital
            // flight. A fixed 10 m ceiling quantizes distant land and ocean
            // into the same depth values and produces striped intersections.
            (distance * 0.01).max(0.05) as f32
        } else {
            0.05
        }
    }
    pub fn terrain(&self) -> &HashMap<CatalogKey, TerrainBuffers> {
        &self.terrain
    }
    pub fn catalog(&self) -> &HashMap<CatalogKey, (wgpu::Buffer, u32)> {
        &self.catalog
    }

    pub fn versions(
        &self,
    ) -> &HashMap<WorldInstanceId, (GenerationRevision, WorldContentRevision)> {
        &self.versions
    }

    pub fn bounds(&self) -> &HashMap<CatalogKey, ShadowBounds> {
        &self.bounds
    }

    pub fn contains(&self, instance: WorldInstanceId, key: SectorKey, lod: u8) -> bool {
        self.catalog.contains_key(&(instance, key, lod))
    }
}
