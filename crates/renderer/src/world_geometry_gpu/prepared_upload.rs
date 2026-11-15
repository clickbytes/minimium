//! Budgeted staging; visible buffers change only at complete scene publication.
use super::*;
use crate::prepared_sector::PreparedSector;

mod publication;

pub(super) struct PendingGeometry {
    shadow: Option<wgpu::Buffer>,
    surface: Option<wgpu::Buffer>,
    ocean: Option<wgpu::Buffer>,
    objects: Option<wgpu::Buffer>,
    source: Arc<PreparedSector>,
    pub origin_generation: u64,
}

impl PendingGeometry {
    pub fn buffers(&self) -> impl Iterator<Item = (&wgpu::Buffer, RebaseLayout)> {
        [
            (self.shadow.as_ref(), RebaseLayout::Object),
            (self.surface.as_ref(), RebaseLayout::Terrain),
            (self.ocean.as_ref(), RebaseLayout::Water),
            (self.objects.as_ref(), RebaseLayout::Object),
        ]
        .into_iter()
        .filter_map(|(buffer, layout)| buffer.map(|buffer| (buffer, layout)))
    }

    pub fn bytes(&self) -> u64 {
        self.buffers().map(|(buffer, _)| buffer.size()).sum()
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
        origin_generation: u64,
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
        self.pending.retain(|(owner, key, lod), pending| {
            let keep = *owner != instance
                || (pending.origin_generation == origin_generation
                    && required
                        .get(&(*key, *lod))
                        .is_some_and(|next| Arc::ptr_eq(next, &pending.source)));
            if !keep {
                released += pending.bytes();
            }
            keep
        });
        self.catalog.retain(|(owner, key, lod), (buffer, _)| {
            let keep = *owner != instance
                || required.contains_key(&(*key, *lod))
                || protected.contains(&(*key, *lod));
            if !keep {
                released += buffer.size();
            }
            keep
        });
        released += self.retain_resident_channels();
        let mut allocated = 0;
        for patch in patches {
            let key = (instance, patch.key, patch.lod);
            let current_origin =
                self.catalog_origin_generation.get(&key) == Some(&origin_generation);
            let previous = self.prepared_sources.get(&key).filter(|_| current_origin);
            let current = previous.is_some_and(|source| Arc::ptr_eq(source, patch));
            let staged = self.pending.get(&key).is_some_and(|pending| {
                pending.origin_generation == origin_generation
                    && Arc::ptr_eq(&pending.source, patch)
            });
            if current || staged {
                continue;
            }
            if self.budget.patches == 0 || self.budget.bytes == 0 {
                break;
            }
            let shadow_changed = !previous.is_some_and(|p| Arc::ptr_eq(&p.shadow, &patch.shadow));
            let surface_changed =
                !previous.is_some_and(|p| Arc::ptr_eq(&p.surface, &patch.surface));
            let ocean_changed = !previous.is_some_and(|p| Arc::ptr_eq(&p.ocean, &patch.ocean));
            let objects_changed =
                !previous.is_some_and(|p| Arc::ptr_eq(&p.objects, &patch.objects));
            let make = |label, bytes: &[u8]| {
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents: if bytes.is_empty() { &[0; 4] } else { bytes },
                    usage: self.usage(),
                })
            };
            let next = PendingGeometry {
                shadow: shadow_changed
                    .then(|| make("prepared shadow", bytemuck::cast_slice(&patch.shadow))),
                surface: (patch.terrain && surface_changed)
                    .then(|| make("prepared terrain", bytemuck::cast_slice(&patch.surface))),
                ocean: (patch.terrain && ocean_changed)
                    .then(|| make("prepared ocean", bytemuck::cast_slice(&patch.ocean))),
                objects: (patch.terrain && objects_changed)
                    .then(|| make("prepared objects", bytemuck::cast_slice(&patch.objects))),
                source: Arc::clone(patch),
                origin_generation,
            };
            let bytes = next.bytes();
            allocated += bytes;
            self.budget.patches -= 1;
            // As in the existing uploader, one oversized patch consumes the
            // entire frame allowance; subsequent patches wait for another frame.
            self.budget.bytes = self.budget.bytes.saturating_sub(bytes);
            self.pending.insert(key, next);
        }
        let ready = !patches.is_empty()
            && patches.iter().all(|patch| {
                let key = (instance, patch.key, patch.lod);
                let active = self.prepared_sources.get(&key).is_some_and(|source| {
                    Arc::ptr_eq(source, patch)
                        && self.catalog_origin_generation.get(&key) == Some(&origin_generation)
                });
                let staged = self.pending.get(&key).is_some_and(|pending| {
                    pending.origin_generation == origin_generation
                        && Arc::ptr_eq(&pending.source, patch)
                });
                active || staged
            });
        GeometryCacheResult {
            ready,
            allocated_bytes: allocated,
            released_bytes: released,
        }
    }

    fn retain_resident_channels(&mut self) -> u64 {
        let mut released = 0;
        self.terrain.retain(|key, buffers| {
            let keep = self.catalog.contains_key(key);
            if !keep {
                released += buffers.bytes();
            }
            keep
        });
        self.sources.retain(|key, _| self.catalog.contains_key(key));
        self.prepared_sources
            .retain(|key, _| self.catalog.contains_key(key));
        self.bounds.retain(|key, _| self.catalog.contains_key(key));
        self.catalog_origin_generation
            .retain(|key, _| self.catalog.contains_key(key));
        released
    }
}
