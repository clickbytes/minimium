use super::*;

impl WorldGeometryGpu {
    pub(in crate::world_geometry_gpu) fn publish_prepared(
        &mut self,
        instance: WorldInstanceId,
        stage: GeometryStageId,
        keys: &[(SectorKey, u8)],
    ) -> bool {
        let mut changed = false;
        let mut released = 0;
        for &(sector, lod) in keys {
            let key = (instance, sector, lod);
            let Some(mut next) = self.pending.remove(&(instance, stage, sector, lod)) else {
                continue;
            };
            let source = &next.source;
            if let Some(shadow) = next.shadow.take() {
                changed = true;
                if let Some((old, _)) = self
                    .catalog
                    .insert(key, (shadow, source.shadow.len() as u32))
                {
                    released += old.size();
                }
            }
            if source.terrain {
                if let Some(current) = self.terrain.get_mut(&key) {
                    for (old, replacement) in [
                        (&mut current.surface, next.surface.take()),
                        (&mut current.ocean, next.ocean.take()),
                        (&mut current.objects, next.objects.take()),
                    ] {
                        if let Some(replacement) = replacement {
                            released += old.size();
                            *old = replacement;
                        }
                    }
                    current.surface_count = source.surface.len() as u32;
                    current.ocean_count = source.ocean.len() as u32;
                    current.object_count = source.objects.len() as u32;
                } else {
                    // Initial staging always prepares every channel together.
                    self.terrain.insert(
                        key,
                        TerrainBuffers {
                            surface: next.surface.take().expect("initial terrain surface"),
                            ocean: next.ocean.take().expect("initial terrain ocean"),
                            objects: next.objects.take().expect("initial terrain objects"),
                            surface_count: source.surface.len() as u32,
                            ocean_count: source.ocean.len() as u32,
                            object_count: source.objects.len() as u32,
                        },
                    );
                }
            } else if let Some(old) = self.terrain.remove(&key) {
                released += old.bytes();
            }
            self.bounds.remove(&key);
            if let Some(bounds) = source.bounds {
                self.bounds.insert(key, bounds);
            }
            self.sources.insert(key, source.geometry.clone());
            self.prepared_sources.insert(key, next.source);
            self.catalog_origin_generation
                .insert(key, next.origin_generation);
        }
        let required: HashSet<_> = keys.iter().copied().collect();
        self.catalog.retain(|(owner, sector, lod), (buffer, _)| {
            let keep = *owner != instance || required.contains(&(*sector, *lod));
            if !keep {
                released += buffer.size();
            }
            keep
        });
        released += self.retain_resident_channels();
        self.released_publication_bytes += released;
        changed
    }

    pub(in crate::world_geometry_gpu) fn publish_raw(
        &mut self,
        instance: WorldInstanceId,
        stage: GeometryStageId,
        keys: &[(SectorKey, u8)],
    ) -> bool {
        let mut changed = false;
        let mut released = 0;
        for &(sector, lod) in keys {
            let staged_key = (instance, stage, sector, lod);
            let Some(next) = self.pending_raw.remove(&staged_key) else {
                continue;
            };
            let key = (instance, sector, lod);
            if let Some((old, _)) = self.catalog.insert(key, (next.buffer, next.count)) {
                released += old.size();
            }
            self.catalog_origin_generation
                .insert(key, next.origin_generation);
            if let Some(old) = self.terrain.remove(&key) {
                released += old.bytes();
            }
            self.bounds.remove(&key);
            if let Some(bounds) = crate::shadow_geometry::geometry_bounds(&next.source) {
                self.bounds.insert(key, bounds);
            }
            self.sources.insert(key, next.source);
            self.prepared_sources.remove(&key);
            changed = true;
        }
        let required: HashSet<_> = keys.iter().copied().collect();
        self.catalog.retain(|(owner, sector, lod), (buffer, _)| {
            if *owner != instance || required.contains(&(*sector, *lod)) {
                true
            } else {
                released += buffer.size();
                false
            }
        });
        released += self.retain_resident_channels();
        self.released_publication_bytes += released;
        self.pending_raw
            .retain(|(owner, candidate, _, _), _| *owner != instance || *candidate != stage);
        changed
    }
}
