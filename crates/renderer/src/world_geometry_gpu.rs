//! GPU sector-geometry buffers and their shadow bounds cache.
use crate::{geometry, shadow_geometry::ShadowBounds};
use minimium_procgen::SectorGeometry;
use minimium_world::world::{GenerationRevision, SectorKey, WorldContentRevision, WorldInstanceId};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use wgpu::util::DeviceExt;

mod prepared_upload;

type CatalogKey = (WorldInstanceId, SectorKey, u8);

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
    pending: HashMap<CatalogKey, prepared_upload::PendingGeometry>,
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
            patches: 8,
            bytes: 4 * 1024 * 1024,
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
        fingerprint: u64,
        keys: &[(SectorKey, u8)],
    ) -> bool {
        let Some((generation, content_revision)) = self.versions.get(&instance).copied() else {
            return false;
        };
        let complete = !keys.is_empty()
            && keys.iter().all(|(key, lod)| {
                let catalog_key = (instance, *key, *lod);
                self.catalog.contains_key(&catalog_key)
                    && self.catalog_origin_generation.get(&catalog_key)
                        == Some(&self.origin_generation)
                    && !self.pending.contains_key(&catalog_key)
            });
        if !complete {
            self.publication = None;
            return false;
        }
        if self.publication.as_ref().is_some_and(|publication| {
            publication.stamp.instance == instance
                && publication.stamp.generation == generation
                && publication.stamp.content_revision == content_revision
                && publication.stamp.world_fingerprint == fingerprint
                && publication.stamp.origin_generation == self.origin_generation
                && publication.keys == keys
        }) {
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
            keys: keys.to_vec(),
        });
        true
    }

    pub fn begin_frame(&mut self) {
        self.budget = UploadBudget::default();
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
        let flat = self.catalog.values().map(|(b, _)| (b, 9));
        let terrain = self
            .terrain
            .values()
            .flat_map(|t| [(&t.surface, 28), (&t.ocean, 6), (&t.objects, 9)]);
        let pending_flat = self.pending.values().map(|p| (&p.shadow, 9));
        let pending_terrain = self
            .pending
            .values()
            .filter_map(|p| p.terrain.as_ref())
            .flat_map(|t| [(&t.surface, 28), (&t.ocean, 6), (&t.objects, 9)]);
        rebase.apply(
            device,
            queue,
            delta,
            flat.chain(terrain)
                .chain(pending_flat)
                .chain(pending_terrain),
        );
        self.origin_generation = origin_generation;
        self.catalog_origin_generation
            .values_mut()
            .for_each(|generation| *generation = origin_generation);
        for pending in self.pending.values_mut() {
            pending.origin_generation = origin_generation;
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

    pub fn cache_scene(
        &mut self,
        device: &wgpu::Device,
        instance: WorldInstanceId,
        generation: GenerationRevision,
        content_revision: WorldContentRevision,
        sectors: &[(SectorKey, u8, Arc<SectorGeometry>)],
        render_origin: [f32; 3],
        terrain: Option<&minimium_procgen::PlanetTerrain>,
        origin_generation: u64,
    ) -> GeometryCacheResult {
        let mut edges = crate::terrain_edges::EdgeConformance::default();
        let conformed = terrain.map(|t| edges.prepare(sectors.to_vec(), t));
        let sectors = conformed.as_deref().unwrap_or(sectors);
        let mut released_bytes = 0_u64;
        let version = (generation, content_revision);
        if self
            .publication
            .as_ref()
            .is_some_and(|publication| publication.stamp.instance == instance)
        {
            self.publication = None;
        }
        if self
            .versions
            .get(&instance)
            .is_some_and(|cached| *cached != version)
        {
            released_bytes += self.release_world(instance);
        }
        self.versions.insert(instance, version);
        let required: HashSet<_> = sectors.iter().map(|(key, lod, _)| (*key, *lod)).collect();
        let mut allocated_bytes = 0_u64;
        for (key, lod, geometry) in sectors {
            let catalog_key = (instance, *key, *lod);
            if self.catalog.contains_key(&catalog_key) {
                continue;
            }
            let vertices = geometry::sector_geometry_relative(geometry, render_origin);
            if let Some(terrain) = terrain {
                let surface = crate::terrain_geometry::vertices(geometry, terrain, render_origin);
                let ocean = edges.ocean(*key, *lod).map_or_else(Vec::new, |g| {
                    crate::ocean_geometry::vertices(g, render_origin)
                });
                let object_count = geometry.boxes.len() * 36;
                let make_buffer = |label, bytes: &[u8]| {
                    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(label),
                        contents: if bytes.is_empty() { &[0; 4] } else { bytes },
                        usage: self.usage(),
                    })
                };
                let buffers = TerrainBuffers {
                    surface: make_buffer("terrain-vertices", bytemuck::cast_slice(&surface)),
                    surface_count: surface.len() as u32,
                    ocean: make_buffer("ocean-vertices", bytemuck::cast_slice(&ocean)),
                    ocean_count: ocean.len() as u32,
                    objects: make_buffer(
                        "terrain-objects",
                        bytemuck::cast_slice(&vertices[..object_count]),
                    ),
                    object_count: object_count as u32,
                };
                allocated_bytes += buffers.bytes();
                self.terrain.insert(catalog_key, buffers);
            }
            let vertices = if terrain.is_some() {
                crate::terrain_geometry::shadow_vertices(geometry, vertices)
            } else {
                vertices
            };
            let empty_vertices = [0.0_f32; 9];
            let contents = if vertices.is_empty() {
                bytemuck::cast_slice(&empty_vertices)
            } else {
                bytemuck::cast_slice(&vertices)
            };
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("world-sector-geometry"),
                contents,
                usage: self.usage(),
            });
            allocated_bytes = allocated_bytes.saturating_add(buffer.size());
            self.catalog
                .insert(catalog_key, (buffer, vertices.len() as u32));
            self.catalog_origin_generation
                .insert(catalog_key, origin_generation);
            if let Some(bounds) = crate::shadow_geometry::geometry_bounds(geometry) {
                self.bounds.insert(catalog_key, bounds);
            }
        }
        self.catalog.retain(|(owner, key, lod), (buffer, _)| {
            if *owner != instance || required.contains(&(*key, *lod)) {
                true
            } else {
                released_bytes = released_bytes.saturating_add(buffer.size());
                false
            }
        });
        self.catalog_origin_generation
            .retain(|key, _| self.catalog.contains_key(key));
        self.bounds
            .retain(|(owner, key, lod), _| *owner != instance || required.contains(&(*key, *lod)));
        self.terrain.retain(|(owner, key, lod), buffers| {
            let retain = *owner != instance || required.contains(&(*key, *lod));
            if !retain {
                released_bytes += buffers.bytes();
            }
            retain
        });
        let ready = sectors
            .iter()
            .all(|(key, lod, _)| self.catalog.contains_key(&(instance, *key, *lod)));
        GeometryCacheResult {
            ready,
            allocated_bytes,
            released_bytes,
        }
    }

    pub fn release_world(&mut self, instance: WorldInstanceId) -> u64 {
        if self
            .publication
            .as_ref()
            .is_some_and(|publication| publication.stamp.instance == instance)
        {
            self.publication = None;
        }
        let mut released_bytes = 0_u64;
        self.catalog.retain(|(owner, _, _), (buffer, _)| {
            if *owner != instance {
                true
            } else {
                released_bytes = released_bytes.saturating_add(buffer.size());
                false
            }
        });
        self.pending.retain(|(owner, _, _), pending| {
            if *owner == instance {
                released_bytes += pending.bytes();
                false
            } else {
                true
            }
        });
        self.versions.remove(&instance);
        self.catalog_origin_generation
            .retain(|(owner, _, _), _| *owner != instance);
        self.sources.retain(|(owner, _, _), _| *owner != instance);
        self.bounds.retain(|(owner, _, _), _| *owner != instance);
        self.terrain.retain(|(owner, _, _), buffers| {
            if *owner != instance {
                true
            } else {
                released_bytes += buffers.bytes();
                false
            }
        });
        released_bytes
    }

    pub fn clear(&mut self) -> u64 {
        self.publication = None;
        let released_bytes = self
            .catalog
            .values()
            .map(|(buffer, _)| buffer.size())
            .sum::<u64>()
            + self
                .terrain
                .values()
                .map(TerrainBuffers::bytes)
                .sum::<u64>();
        let released_bytes = released_bytes + self.pending.values().map(|p| p.bytes()).sum::<u64>();
        self.pending.clear();
        self.sources.clear();
        self.terrain.clear();
        self.catalog.clear();
        self.catalog_origin_generation.clear();
        self.versions.clear();
        self.bounds.clear();
        released_bytes
    }
}
