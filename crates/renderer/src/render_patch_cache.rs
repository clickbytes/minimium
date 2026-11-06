//! Bounded CPU geometry cache; caller resets it for a different world/version.
use minimium_procgen::SectorGeometry;
use minimium_world::world::SectorKey;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
#[derive(Default)]
pub struct RenderPatchCache {
    entries: HashMap<(SectorKey, u8), (u64, Arc<SectorGeometry>)>,
    tick: u64,
    pub refined: HashSet<(minimium_world::world::PlanetFace, u16, u16, u16)>,
    pub(crate) coasts: crate::coast_probe::CoastProbeCache,
}
impl RenderPatchCache {
    pub fn bytes(&self) -> usize {
        self.entries
            .values()
            .map(|(_, g)| {
                g.triangles.capacity() * std::mem::size_of::<minimium_world::world::MapTriangle>()
                    + g.boxes.capacity() * std::mem::size_of::<minimium_world::world::MapBox>()
            })
            .sum()
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
