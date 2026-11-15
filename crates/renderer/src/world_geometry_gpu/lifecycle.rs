use super::*;

impl WorldGeometryGpu {
    pub fn release_world(&mut self, instance: WorldInstanceId) -> u64 {
        if self
            .publication
            .as_ref()
            .is_some_and(|publication| publication.stamp.instance == instance)
        {
            self.publication = None;
        }
        let mut released_bytes = 0_u64;
        if let Some(water) = self.water.remove(&instance) {
            released_bytes += water.bytes();
        }
        self.catalog.retain(|(owner, _, _), (buffer, _)| {
            if *owner != instance {
                true
            } else {
                released_bytes = released_bytes.saturating_add(buffer.size());
                false
            }
        });
        self.pending.retain(|(owner, _, _, _), pending| {
            if *owner == instance {
                released_bytes += pending.bytes();
                false
            } else {
                true
            }
        });
        self.pending_raw.retain(|(owner, _, _, _), pending| {
            if *owner == instance {
                released_bytes += pending.bytes();
                false
            } else {
                true
            }
        });
        self.staged_versions
            .retain(|(owner, _), _| *owner != instance);
        self.versions.remove(&instance);
        self.catalog_origin_generation
            .retain(|(owner, _, _), _| *owner != instance);
        self.sources.retain(|(owner, _, _), _| *owner != instance);
        self.prepared_sources
            .retain(|(owner, _, _), _| *owner != instance);
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
        let released_bytes = released_bytes
            + self.pending.values().map(|p| p.bytes()).sum::<u64>()
            + self.pending_raw.values().map(|p| p.bytes()).sum::<u64>()
            + self.water.values().map(|slot| slot.bytes()).sum::<u64>()
            + self.take_released_publication_bytes();
        self.water.clear();
        self.pending.clear();
        self.pending_raw.clear();
        self.staged_versions.clear();
        self.sources.clear();
        self.prepared_sources.clear();
        self.terrain.clear();
        self.catalog.clear();
        self.catalog_origin_generation.clear();
        self.versions.clear();
        self.bounds.clear();
        released_bytes
    }
}
