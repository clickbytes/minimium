use super::*;
use crate::ocean_geometry::OceanVertex;

struct LocalWaterImage {
    source: Arc<Vec<OceanVertex>>,
    buffer: Option<wgpu::Buffer>,
    origin_generation: u64,
}

#[derive(Default)]
pub(super) struct LocalWaterSlot {
    active: Option<LocalWaterImage>,
    pending: Option<LocalWaterImage>,
}

impl LocalWaterSlot {
    pub(super) fn buffers(&self) -> impl Iterator<Item = &wgpu::Buffer> {
        self.active
            .iter()
            .chain(self.pending.iter())
            .filter_map(|image| image.buffer.as_ref())
    }

    pub(super) fn bytes(&self) -> u64 {
        self.buffers().map(wgpu::Buffer::size).sum()
    }

    pub(super) fn rebase(&mut self, generation: u64) {
        for image in self.active.iter_mut().chain(self.pending.iter_mut()) {
            image.origin_generation = generation;
        }
    }
}

impl WorldGeometryGpu {
    /// Stages the worker-prepared local surface against the same frame budget
    /// as terrain. It becomes visible only when publish_scene installs the set.
    pub fn cache_local_water(
        &mut self,
        device: &wgpu::Device,
        instance: WorldInstanceId,
        source: Arc<Vec<OceanVertex>>,
        origin_generation: u64,
    ) -> GeometryCacheResult {
        let matches = |image: &LocalWaterImage| {
            image.origin_generation == origin_generation && Arc::ptr_eq(&image.source, &source)
        };
        if let Some(slot) = self.water.get(&instance) {
            if slot.pending.as_ref().is_some_and(matches)
                || slot.pending.is_none() && slot.active.as_ref().is_some_and(matches)
            {
                return GeometryCacheResult {
                    ready: true,
                    allocated_bytes: 0,
                    released_bytes: 0,
                };
            }
        }
        let bytes = (source.len() * std::mem::size_of::<OceanVertex>()) as u64;
        if origin_generation != self.origin_generation
            || bytes > self.budget.bytes
            || bytes > 0 && self.budget.patches == 0
        {
            return GeometryCacheResult {
                ready: false,
                allocated_bytes: 0,
                released_bytes: 0,
            };
        }
        let buffer = (!source.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("local-water-surface"),
                contents: bytemuck::cast_slice(&source),
                usage: self.usage(),
            })
        });
        if bytes > 0 {
            self.budget.bytes -= bytes;
            self.budget.patches -= 1;
        }
        let slot = self.water.entry(instance).or_default();
        let released_bytes = slot
            .pending
            .as_ref()
            .and_then(|image| image.buffer.as_ref())
            .map_or(0, wgpu::Buffer::size);
        slot.pending = Some(LocalWaterImage {
            source,
            buffer,
            origin_generation,
        });
        GeometryCacheResult {
            ready: true,
            allocated_bytes: bytes,
            released_bytes,
        }
    }

    pub(super) fn publish_local_water(&mut self, instance: WorldInstanceId) -> bool {
        let Some(slot) = self.water.get_mut(&instance) else {
            return false;
        };
        let Some(pending) = slot.pending.take() else {
            return false;
        };
        self.released_publication_bytes += slot
            .active
            .as_ref()
            .and_then(|image| image.buffer.as_ref())
            .map_or(0, wgpu::Buffer::size);
        slot.active = Some(pending);
        true
    }

    pub fn take_released_publication_bytes(&mut self) -> u64 {
        std::mem::take(&mut self.released_publication_bytes)
    }

    pub fn local_water(&self, instance: WorldInstanceId) -> (Option<&wgpu::Buffer>, u32) {
        self.water
            .get(&instance)
            .and_then(|slot| slot.active.as_ref())
            .map_or((None, 0), |image| {
                (image.buffer.as_ref(), image.source.len() as u32)
            })
    }
}
