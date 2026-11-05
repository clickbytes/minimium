//! GPU sector-geometry buffers and their shadow bounds cache.
use crate::{geometry, shadow_geometry::ShadowBounds};
use minimium_procgen::SectorGeometry;
use minimium_world::world::{GenerationRevision, SectorKey, WorldContentRevision, WorldInstanceId};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
use wgpu::util::DeviceExt;

type CatalogKey = (WorldInstanceId, SectorKey, u8);

/// Owns per-world sector vertex buffers and geometry bounds used by shadow selection.
#[derive(Default)]
pub struct WorldGeometryGpu {
    catalog: HashMap<CatalogKey, (wgpu::Buffer, u32)>,
    versions: HashMap<WorldInstanceId, (GenerationRevision, WorldContentRevision)>,
    bounds: HashMap<CatalogKey, ShadowBounds>,
}

/// Result of reconciling visible geometry against the current GPU cache.
pub struct GeometryCacheResult {
    pub ready: bool,
    pub allocated_bytes: u64,
    pub released_bytes: u64,
}

impl WorldGeometryGpu {
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
    ) -> GeometryCacheResult {
        let mut released_bytes = 0_u64;
        let version = (generation, content_revision);
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
            let empty_vertices = [0.0_f32; 9];
            let contents = if vertices.is_empty() {
                bytemuck::cast_slice(&empty_vertices)
            } else {
                bytemuck::cast_slice(&vertices)
            };
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("world-sector-geometry"),
                contents,
                usage: wgpu::BufferUsages::VERTEX,
            });
            allocated_bytes = allocated_bytes.saturating_add(buffer.size());
            self.catalog
                .insert(catalog_key, (buffer, vertices.len() as u32));
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
        self.bounds
            .retain(|(owner, key, lod), _| *owner != instance || required.contains(&(*key, *lod)));
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
        let mut released_bytes = 0_u64;
        self.catalog.retain(|(owner, _, _), (buffer, _)| {
            if *owner != instance {
                true
            } else {
                released_bytes = released_bytes.saturating_add(buffer.size());
                false
            }
        });
        self.versions.remove(&instance);
        self.bounds.retain(|(owner, _, _), _| *owner != instance);
        released_bytes
    }

    pub fn clear(&mut self) -> u64 {
        let released_bytes = self.catalog.values().map(|(buffer, _)| buffer.size()).sum();
        self.catalog.clear();
        self.versions.clear();
        self.bounds.clear();
        released_bytes
    }
}
