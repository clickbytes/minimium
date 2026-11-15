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

pub(super) struct PendingRawGeometry {
    pub buffer: wgpu::Buffer,
    pub count: u32,
    pub source: Arc<SectorGeometry>,
    pub origin_generation: u64,
}

impl PendingRawGeometry {
    pub fn bytes(&self) -> u64 {
        self.buffer.size()
    }
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
        stage: GeometryStageId,
        generation: GenerationRevision,
        content_revision: WorldContentRevision,
        patches: &[Arc<PreparedSector>],
        active: &[(SectorKey, u8)],
        origin_generation: u64,
    ) -> GeometryCacheResult {
        let version = (generation, content_revision);
        let stage_key = (instance, stage);
        let mut released = 0;
        if self
            .staged_versions
            .get(&stage_key)
            .is_some_and(|previous| *previous != version)
        {
            released += self.release_scene_stage(instance, stage);
        }
        if !self.can_stage(instance, stage) {
            return GeometryCacheResult {
                ready: false,
                allocated_bytes: 0,
                released_bytes: released,
            };
        }
        self.register_stage(instance, stage);
        self.staged_versions.insert(stage_key, version);
        let is_published_stage = self.publication.as_ref().is_some_and(|publication| {
            publication.stamp.instance == instance && publication.stage == stage
        });
        let required: HashMap<_, _> = patches.iter().map(|p| ((p.key, p.lod), p)).collect();
        let protected: HashSet<_> = active.iter().copied().collect();
        self.pending
            .retain(|(owner, candidate, key, lod), pending| {
                let keep = *owner != instance
                    || *candidate != stage
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
            let previous = self
                .prepared_sources
                .get(&key)
                .filter(|_| current_origin && self.versions.get(&instance) == Some(&version))
                .cloned();
            let current = previous
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, patch));
            let stage_key = (instance, stage, patch.key, patch.lod);
            let staged = self.pending.get(&stage_key).is_some_and(|pending| {
                pending.origin_generation == origin_generation
                    && Arc::ptr_eq(&pending.source, patch)
            });
            if current && is_published_stage || staged {
                continue;
            }
            if self.budget.patches == 0 || self.budget.bytes == 0 {
                break;
            }
            if !self.stage_can_upload(instance, stage) {
                break;
            }
            let shadow_changed = !is_published_stage
                || !previous
                    .as_ref()
                    .is_some_and(|p| Arc::ptr_eq(&p.shadow, &patch.shadow));
            let surface_changed = !is_published_stage
                || !previous
                    .as_ref()
                    .is_some_and(|p| Arc::ptr_eq(&p.surface, &patch.surface));
            let ocean_changed = !is_published_stage
                || !previous
                    .as_ref()
                    .is_some_and(|p| Arc::ptr_eq(&p.ocean, &patch.ocean));
            let objects_changed = !is_published_stage
                || !previous
                    .as_ref()
                    .is_some_and(|p| Arc::ptr_eq(&p.objects, &patch.objects));
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
            self.pending.insert(stage_key, next);
        }
        let ready = !patches.is_empty()
            && patches.iter().all(|patch| {
                let key = (instance, patch.key, patch.lod);
                let active = is_published_stage
                    && self.prepared_sources.get(&key).is_some_and(|source| {
                        Arc::ptr_eq(source, patch)
                            && self.versions.get(&instance) == Some(&version)
                            && self.catalog_origin_generation.get(&key) == Some(&origin_generation)
                    });
                let staged = self
                    .pending
                    .get(&(instance, stage, patch.key, patch.lod))
                    .is_some_and(|pending| {
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

    pub fn cache_scene_staged(
        &mut self,
        device: &wgpu::Device,
        instance: WorldInstanceId,
        stage: GeometryStageId,
        generation: GenerationRevision,
        content_revision: WorldContentRevision,
        sectors: &[(SectorKey, u8, Arc<SectorGeometry>)],
        render_origin: [f32; 3],
        origin_generation: u64,
    ) -> GeometryCacheResult {
        let version = (generation, content_revision);
        let stage_key = (instance, stage);
        let mut released = 0;
        if self
            .staged_versions
            .get(&stage_key)
            .is_some_and(|previous| *previous != version)
        {
            released += self.release_scene_stage(instance, stage);
        }
        if !self.can_stage(instance, stage) {
            return GeometryCacheResult {
                ready: false,
                allocated_bytes: 0,
                released_bytes: released,
            };
        }
        self.register_stage(instance, stage);
        self.staged_versions.insert(stage_key, version);
        let is_published_stage = self.publication.as_ref().is_some_and(|publication| {
            publication.stamp.instance == instance && publication.stage == stage
        });
        let required = sectors
            .iter()
            .map(|(key, lod, _)| (*key, *lod))
            .collect::<HashSet<_>>();
        self.pending_raw
            .retain(|(owner, candidate, key, lod), pending| {
                let keep = *owner != instance
                    || *candidate != stage
                    || (required.contains(&(*key, *lod))
                        && pending.origin_generation == origin_generation
                        && sectors.iter().any(|(next_key, next_lod, next)| {
                            next_key == key && next_lod == lod && Arc::ptr_eq(next, &pending.source)
                        }));
                if !keep {
                    released += pending.bytes();
                }
                keep
            });
        let mut allocated = 0;
        for (key, lod, source) in sectors {
            let catalog_key = (instance, *key, *lod);
            let already_active = is_published_stage
                && self.versions.get(&instance) == Some(&version)
                && self.sources.get(&catalog_key).is_some_and(|current| {
                    Arc::ptr_eq(current, source)
                        && self.catalog_origin_generation.get(&catalog_key)
                            == Some(&origin_generation)
                });
            let staged_key = (instance, stage, *key, *lod);
            let already_staged = self.pending_raw.get(&staged_key).is_some_and(|pending| {
                pending.origin_generation == origin_generation
                    && Arc::ptr_eq(&pending.source, source)
            });
            if already_active || already_staged {
                continue;
            }
            if self.budget.patches == 0 || self.budget.bytes == 0 {
                break;
            }
            if !self.stage_can_upload(instance, stage) {
                break;
            }
            let vertices = geometry::sector_geometry_relative(source, render_origin);
            let empty_vertices = [0.0_f32; 9];
            let contents = if vertices.is_empty() {
                bytemuck::cast_slice(&empty_vertices)
            } else {
                bytemuck::cast_slice(&vertices)
            };
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("staged-world-sector-geometry"),
                contents,
                usage: self.usage(),
            });
            let bytes = buffer.size();
            allocated += bytes;
            self.budget.patches -= 1;
            self.budget.bytes = self.budget.bytes.saturating_sub(bytes);
            self.pending_raw.insert(
                staged_key,
                PendingRawGeometry {
                    buffer,
                    count: vertices.len() as u32,
                    source: Arc::clone(source),
                    origin_generation,
                },
            );
        }
        let ready = !sectors.is_empty()
            && sectors.iter().all(|(key, lod, source)| {
                let catalog_key = (instance, *key, *lod);
                let active = is_published_stage
                    && self.versions.get(&instance) == Some(&version)
                    && self.sources.get(&catalog_key).is_some_and(|current| {
                        Arc::ptr_eq(current, source)
                            && self.catalog_origin_generation.get(&catalog_key)
                                == Some(&origin_generation)
                    });
                let staged = self
                    .pending_raw
                    .get(&(instance, stage, *key, *lod))
                    .is_some_and(|pending| {
                        pending.origin_generation == origin_generation
                            && Arc::ptr_eq(&pending.source, source)
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
