//! Bounded CPU geometry cache scoped to one world instance and manifest.
use minimium_procgen::{SectorContentVersion, SectorGeometry};
use minimium_world::world::{SectorKey, WorldInstanceId, WorldManifest};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
#[derive(Default)]
pub struct RenderPatchCache {
    context: Option<(WorldInstanceId, WorldManifest)>,
    revisions: HashMap<(SectorKey, u8), SectorContentVersion>,
    entries: HashMap<(SectorKey, u8), (SectorContentVersion, Arc<SectorGeometry>)>,
    base_entries: HashMap<(SectorKey, u8), (SectorContentVersion, u64, Arc<SectorGeometry>)>,
    tick: u64,
    pub refined: HashSet<(minimium_world::world::PlanetFace, u16, u16, u16)>,
    pub(crate) coasts: crate::coast_probe::CoastProbeCache,
}
impl RenderPatchCache {
    /// Bind cached geometry to a world instance and its immutable manifest.
    /// Reusing a cache for another context drops every context-derived entry.
    pub fn set_context(&mut self, world: WorldInstanceId, manifest: WorldManifest) {
        if self.context == Some((world, manifest)) {
            return;
        }
        self.context = Some((world, manifest));
        self.revisions.clear();
        self.entries.clear();
        self.base_entries.clear();
        self.refined.clear();
        self.coasts = Default::default();
        self.tick = 0;
    }

    /// Bounded cache for the unmodified LOD meshes used to construct exact
    /// excavation contours before final meshes replace their chunk domains.
    pub fn get_base_revision(
        &mut self,
        key: SectorKey,
        lod: u8,
        revision: SectorContentVersion,
        build: impl FnOnce() -> SectorGeometry,
    ) -> Arc<SectorGeometry> {
        self.tick = self.tick.wrapping_add(1);
        if let Some((current, age, geometry)) = self.base_entries.get_mut(&(key, lod)) {
            if *current == revision {
                *age = self.tick;
                return geometry.clone();
            }
        }
        let geometry = Arc::new(build());
        self.base_entries.remove(&(key, lod));
        while self.base_entries.len() >= 512 || self.base_bytes() >= 64 * 1024 * 1024 {
            let oldest = self
                .base_entries
                .iter()
                .min_by_key(|(_, (_, age, _))| *age)
                .map(|(key, _)| *key);
            let Some(oldest) = oldest else { break };
            self.base_entries.remove(&oldest);
        }
        self.base_entries
            .insert((key, lod), (revision, self.tick, geometry.clone()));
        geometry
    }

    fn base_bytes(&self) -> usize {
        self.base_entries
            .values()
            .map(|(_, _, geometry)| {
                (geometry.triangles.capacity() + geometry.object_triangles.capacity())
                    * std::mem::size_of::<minimium_world::world::MapTriangle>()
                    + geometry.boxes.capacity()
                        * std::mem::size_of::<minimium_world::world::MapBox>()
            })
            .sum()
    }

    pub fn get_revision(
        &mut self,
        key: SectorKey,
        lod: u8,
        revision: SectorContentVersion,
        build: impl FnOnce() -> SectorGeometry,
    ) -> Arc<SectorGeometry> {
        if self.revisions.get(&(key, lod)) != Some(&revision) {
            self.entries.remove(&(key, lod));
        }
        self.revisions.insert((key, lod), revision);
        self.get(key, lod, build)
    }

    /// Fallible build variant. A failed geometry preflight does not replace the
    /// last complete entry or publish an empty/partial mesh under a new revision.
    pub fn get_revision_result<E>(
        &mut self,
        key: SectorKey,
        lod: u8,
        revision: SectorContentVersion,
        build: impl FnOnce() -> Result<SectorGeometry, E>,
    ) -> Result<Arc<SectorGeometry>, E> {
        self.tick = self.tick.wrapping_add(1);
        if self.revisions.get(&(key, lod)) == Some(&revision) {
            if let Some((age, geometry)) = self.entries.get_mut(&(key, lod)) {
                *age = self.tick;
                return Ok(geometry.clone());
            }
        }
        let geometry = Arc::new(build()?);
        self.entries.remove(&(key, lod));
        self.revisions.insert((key, lod), revision);
        while self.entries.len() >= 1024 || self.bytes() >= 128 * 1024 * 1024 {
            let old = self
                .entries
                .iter()
                .filter(|(old, _)| **old != (key, lod))
                .min_by_key(|(_, value)| value.0)
                .map(|(old, _)| *old);
            let Some(old) = old else { break };
            self.entries.remove(&old);
            self.revisions.remove(&old);
        }
        self.entries
            .insert((key, lod), (self.tick, geometry.clone()));
        Ok(geometry)
    }
    pub fn bytes(&self) -> usize {
        self.base_bytes()
            + self
                .entries
                .values()
                .map(|(_, g)| {
                    (g.triangles.capacity() + g.object_triangles.capacity())
                        * std::mem::size_of::<minimium_world::world::MapTriangle>()
                        + g.boxes.capacity() * std::mem::size_of::<minimium_world::world::MapBox>()
                })
                .sum::<usize>()
    }

    pub fn with_boxes(
        &mut self,
        key: SectorKey,
        lod: u8,
        geometry: Arc<SectorGeometry>,
        boxes: Option<&[minimium_world::world::MapBox]>,
    ) -> Arc<SectorGeometry> {
        let Some(boxes) = boxes else {
            return geometry;
        };
        if geometry.boxes.len() == boxes.len()
            && geometry.boxes.iter().zip(boxes).all(|(a, b)| {
                a.center == b.center && a.size == b.size && a.color == b.color && a.solid == b.solid
            })
        {
            return geometry;
        }
        let mut next = (*geometry).clone();
        next.boxes = boxes.to_vec();
        let next = Arc::new(next);
        self.entries.insert((key, lod), (self.tick, next.clone()));
        next
    }
    pub fn get(
        &mut self,
        key: SectorKey,
        lod: u8,
        build: impl FnOnce() -> SectorGeometry,
    ) -> Arc<SectorGeometry> {
        self.tick = self.tick.wrapping_add(1);
        if let Some((age, geometry)) = self.entries.get_mut(&(key, lod)) {
            *age = self.tick;
            return geometry.clone();
        }
        while self.entries.len() >= 1024 || self.bytes() >= 128 * 1024 * 1024 {
            if let Some(old) = self
                .entries
                .iter()
                .min_by_key(|(_, v)| v.0)
                .map(|(k, _)| *k)
            {
                self.entries.remove(&old);
                self.revisions.remove(&old);
            } else {
                break;
            }
        }
        let geometry = Arc::new(build());
        self.entries
            .insert((key, lod), (self.tick, geometry.clone()));
        geometry
    }
}
