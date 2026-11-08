use super::Vertex;
use minimium_renderer::world_geometry_gpu::GeometryPublicationStamp;
use minimium_world::world::{GenerationRevision, WorldContentRevision, WorldInstanceId, WorldMap};
use std::sync::Arc;

pub(super) const MAX_MAP_VERTEX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ShadowMapCacheKey {
    pub(super) stamp: GeometryPublicationStamp,
    pub(super) instance: WorldInstanceId,
    pub(super) generation: GenerationRevision,
    pub(super) content_revision: WorldContentRevision,
    pub(super) fingerprint: u64,
    pub(super) center: [f64; 3],
    pub(super) radius: f32,
    pub(super) render_origin: [f32; 3],
}

#[derive(Clone)]
pub(super) struct PreparedMapCasters {
    pub(super) key: ShadowMapCacheKey,
    pub(super) vertices: Arc<[Vertex]>,
}

pub(super) struct ShadowMapPreparationCache {
    key: Option<ShadowMapCacheKey>,
    vertices: Arc<[Vertex]>,
}

impl Default for ShadowMapPreparationCache {
    fn default() -> Self {
        Self {
            key: None,
            vertices: Vec::new().into(),
        }
    }
}

impl ShadowMapPreparationCache {
    pub(super) fn prepare(
        &mut self,
        stamp: GeometryPublicationStamp,
        instance: WorldInstanceId,
        generation: GenerationRevision,
        content_revision: WorldContentRevision,
        map: &WorldMap,
        center: [f64; 3],
        radius: f32,
        render_origin: [f32; 3],
    ) -> Result<PreparedMapCasters, ()> {
        let step = (f64::from(radius) * 0.0625).max(16.0);
        let center = center.map(|value| (value / step).round() * step);
        let key = ShadowMapCacheKey {
            stamp,
            instance,
            generation,
            content_revision,
            fingerprint: map.fingerprint(),
            center,
            radius,
            render_origin,
        };
        if self.key != Some(key) {
            let geometry = minimium_renderer::shadow_geometry::local_map_geometry(
                map,
                center,
                f64::from(radius) + step,
            );
            let vertices =
                minimium_renderer::geometry::sector_geometry_relative(&geometry, render_origin);
            if std::mem::size_of_val(vertices.as_slice()) as u64 > MAX_MAP_VERTEX_BYTES {
                return Err(());
            }
            self.vertices = vertices.into();
            self.key = Some(key);
        }
        Ok(PreparedMapCasters {
            key,
            vertices: Arc::clone(&self.vertices),
        })
    }

    pub(super) fn release_world(&mut self, instance: WorldInstanceId) {
        if self.key.is_some_and(|key| key.instance == instance) {
            self.key = None;
            self.vertices = Vec::new().into();
        }
    }
}
