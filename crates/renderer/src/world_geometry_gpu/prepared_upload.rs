//! Budgeted staging and atomic publication of a complete presentation selection.
use super::*;
use crate::prepared_sector::PreparedSector;

pub(super) struct PendingGeometry {
    pub shadow: wgpu::Buffer,
    count: u32,
    pub terrain: Option<TerrainBuffers>,
    bounds: Option<ShadowBounds>,
    source: Arc<SectorGeometry>,
}

impl PendingGeometry {
    pub fn bytes(&self) -> u64 {
        self.shadow.size() + self.terrain.as_ref().map_or(0, TerrainBuffers::bytes)
    }
}

impl WorldGeometryGpu {
    pub fn cache_prepared(
        &mut self,
        device: &wgpu::Device,
        instance: WorldInstanceId,
        generation: GenerationRevision,
        content_revision: WorldContentRevision,
        patches: &[Arc<PreparedSector>],
        active: &[(SectorKey, u8)],
    ) -> GeometryCacheResult {
        let version = (generation, content_revision);
        let mut released = if self.versions.get(&instance).is_some_and(|v| *v != version) {
            self.release_world(instance)
        } else {
            0
        };
        self.versions.insert(instance, version);
        let required: HashMap<_, _> = patches.iter().map(|p| ((p.key, p.lod), p)).collect();
        let protected: HashSet<_> = active.iter().copied().collect();
        self.pending.retain(|(owner, key, lod), p| {
            let keep = *owner != instance
                || required
                    .get(&(*key, *lod))
                    .is_some_and(|next| Arc::ptr_eq(&next.geometry, &p.source));
            if !keep {
                released += p.bytes();
            }
            keep
        });
        self.catalog.retain(|(owner, key, lod), (b, _)| {
            let keep = *owner != instance
                || required.contains_key(&(*key, *lod))
                || protected.contains(&(*key, *lod));
            if !keep {
                released += b.size();
            }
            keep
        });
        self.terrain.retain(|k, b| {
            let keep = self.catalog.contains_key(k);
            if !keep {
                released += b.bytes();
            }
            keep
        });
        self.sources.retain(|k, _| self.catalog.contains_key(k));
        self.bounds.retain(|k, _| self.catalog.contains_key(k));
        let mut allocated = 0;
        for patch in patches {
            let key = (instance, patch.key, patch.lod);
            let current = self
                .sources
                .get(&key)
                .is_some_and(|s| Arc::ptr_eq(s, &patch.geometry));
            let staged = self
                .pending
                .get(&key)
                .is_some_and(|s| Arc::ptr_eq(&s.source, &patch.geometry));
            if current || staged {
                continue;
            }
            if self.budget.patches == 0 || self.budget.bytes == 0 {
                break;
            }
            let make = |label, bytes: &[u8]| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: if bytes.is_empty() { &[0; 4] } else { bytes },
                    usage: self.usage(),
                })
            };
            let terrain = patch.terrain.then(|| TerrainBuffers {
                surface: make("prepared terrain", bytemuck::cast_slice(&patch.surface)),
                surface_count: patch.surface.len() as u32,
                ocean: make("prepared ocean", bytemuck::cast_slice(&patch.ocean)),
                ocean_count: patch.ocean.len() as u32,
                objects: make("prepared objects", bytemuck::cast_slice(&patch.objects)),
                object_count: patch.objects.len() as u32,
            });
            let next = PendingGeometry {
                shadow: make(
                    "prepared shadow geometry",
                    bytemuck::cast_slice(&patch.shadow),
                ),
                count: patch.shadow.len() as u32,
                terrain,
                bounds: patch.bounds,
                source: patch.geometry.clone(),
            };
            let bytes = next.bytes();
            allocated += bytes;
            self.budget.patches -= 1;
            self.budget.bytes = self.budget.bytes.saturating_sub(bytes);
            self.pending.insert(key, next);
        }
        let ready = !patches.is_empty()
            && patches.iter().all(|p| {
                let key = (instance, p.key, p.lod);
                self.sources
                    .get(&key)
                    .is_some_and(|s| Arc::ptr_eq(s, &p.geometry))
                    || self
                        .pending
                        .get(&key)
                        .is_some_and(|s| Arc::ptr_eq(&s.source, &p.geometry))
            });
        if ready {
            for patch in patches {
                let key = (instance, patch.key, patch.lod);
                let Some(next) = self.pending.remove(&key) else {
                    continue;
                };
                if let Some((old, _)) = self.catalog.insert(key, (next.shadow, next.count)) {
                    released += old.size();
                }
                if let Some(old) = self.terrain.remove(&key) {
                    released += old.bytes();
                }
                if let Some(terrain) = next.terrain {
                    self.terrain.insert(key, terrain);
                }
                self.bounds.remove(&key);
                if let Some(bounds) = next.bounds {
                    self.bounds.insert(key, bounds);
                }
                self.sources.insert(key, next.source);
            }
            self.catalog.retain(|(owner, key, lod), (b, _)| {
                let keep = *owner != instance || required.contains_key(&(*key, *lod));
                if !keep {
                    released += b.size();
                }
                keep
            });
            self.terrain.retain(|key, b| {
                let keep = self.catalog.contains_key(key);
                if !keep {
                    released += b.bytes();
                }
                keep
            });
            self.sources.retain(|key, _| self.catalog.contains_key(key));
            self.bounds.retain(|key, _| self.catalog.contains_key(key));
        }
        GeometryCacheResult {
            ready,
            allocated_bytes: allocated,
            released_bytes: released,
        }
    }
}
