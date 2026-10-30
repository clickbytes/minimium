use minimium_core::world::{
    Aabb, CollisionIndex, GenerationRevision, MapBox, MapTriangle, SectorKey, SectorRevision,
    WorldContentRevision, WorldInstanceId, WorldManifest, WorldMap,
};
use minimium_procgen::SectorGeometry;
use minimium_sectors::{InstallResult, RequestError, SectorManager, SectorTicket};
use std::{collections::HashMap, sync::Arc};

const CLIENT_SECTOR_CACHE_CAPACITY: usize = 512;
const MAX_CLIENT_INSTALLS_PER_FRAME: usize = 32;

pub(super) struct ClientSectorCache {
    manager: SectorManager<SectorGeometry>,
    interests: HashMap<WorldInstanceId, SectorInterest>,
}

struct SectorInterest {
    keys: Vec<SectorKey>,
    tickets: Vec<SectorTicket>,
    generation: GenerationRevision,
    content_revision: WorldContentRevision,
}

pub(super) struct ClientSectorScene {
    pub(super) keys: Vec<SectorKey>,
    pub(super) sectors: Vec<(SectorKey, Arc<SectorGeometry>)>,
    pub(super) render_sectors: Vec<(SectorKey, u8, Arc<SectorGeometry>)>,
    pub(super) collision: CollisionIndex,
    pub(super) generation: GenerationRevision,
    pub(super) content_revision: WorldContentRevision,
}

impl ClientSectorCache {
    pub(super) fn reset(&mut self) {
        *self = Self::new();
    }
    pub(super) fn new() -> Self {
        Self {
            manager: SectorManager::new(CLIENT_SECTOR_CACHE_CAPACITY, CLIENT_SECTOR_CACHE_CAPACITY)
                .expect("client sector cache capacity is nonzero"),
            interests: HashMap::new(),
        }
    }

    pub(super) fn interest_matches(
        &self,
        instance: WorldInstanceId,
        keys: &[SectorKey],
        manifest: WorldManifest,
    ) -> bool {
        self.interests.get(&instance).is_some_and(|interest| {
            interest.keys == keys
                && interest.tickets.len() == keys.len()
                && interest.generation == manifest.generation().revision()
                && interest.content_revision == manifest.content_revision()
        })
    }

    pub(super) fn ensure_scene(
        &mut self,
        instance: WorldInstanceId,
        manifest: WorldManifest,
        position: [f32; 3],
        maps: &HashMap<WorldInstanceId, WorldMap>,
        manifests: &HashMap<WorldInstanceId, WorldManifest>,
    ) -> Option<ClientSectorScene> {
        if maps.get(&instance).is_none() || manifests.get(&instance) != Some(&manifest) {
            return None;
        }
        let keys = minimium_procgen::sector_neighborhood_for(manifest, position);
        let same_interest = self.interest_matches(instance, &keys, manifest);
        if !same_interest {
            if let Some(previous) = self.interests.remove(&instance) {
                for ticket in previous.tickets {
                    self.manager.unpin(ticket);
                }
            }
            let mut tickets = Vec::with_capacity(keys.len());
            for key in &keys {
                let Some(ticket) = self.request(instance, *key, manifest) else {
                    continue;
                };
                if self.manager.pin(ticket) {
                    tickets.push(ticket);
                }
            }
            self.interests.insert(
                instance,
                SectorInterest {
                    keys: keys.clone(),
                    tickets,
                    generation: manifest.generation().revision(),
                    content_revision: manifest.content_revision(),
                },
            );
        }

        for _ in 0..MAX_CLIENT_INSTALLS_PER_FRAME {
            let Some(ticket) = self.manager.begin_next() else {
                break;
            };
            let Some(source_map) = maps.get(&ticket.instance()) else {
                continue;
            };
            let Some(source_manifest) = manifests.get(&ticket.instance()) else {
                continue;
            };
            let geometry = Arc::new(minimium_procgen::materialize_world_sector(
                source_map,
                *source_manifest,
                ticket.key(),
            ));
            let revision = SectorRevision::new(1).expect("initial sector revision is nonzero");
            if self.manager.install_data(ticket, revision, geometry) == InstallResult::Installed {
                let _ = self.manager.install_collision(ticket);
            }
        }

        let interest = self.interests.get(&instance)?;
        if interest.keys != keys
            || interest.tickets.len() != keys.len()
            || !interest
                .tickets
                .iter()
                .all(|ticket| self.manager.collision_ready(*ticket))
        {
            return None;
        }
        let sectors: Vec<_> = interest
            .tickets
            .iter()
            .map(|ticket| Some((ticket.key(), self.manager.payload_arc(*ticket)?)))
            .collect::<Option<_>>()?;
        let collision =
            collision_from_sectors(sectors.iter().map(|(_, geometry)| geometry.as_ref()));
        let render_sectors = if manifest.topology() == minimium_core::world::WorldTopology::Planet {
            super::planet_visibility::render_patches(manifest, position, &sectors)
        } else {
            sectors
                .iter()
                .map(|(key, geometry)| (*key, 0, Arc::clone(geometry)))
                .collect()
        };
        Some(ClientSectorScene {
            keys,
            sectors,
            render_sectors,
            collision,
            generation: manifest.generation().revision(),
            content_revision: manifest.content_revision(),
        })
    }

    pub(super) fn release_interest(&mut self, instance: WorldInstanceId) {
        if let Some(interest) = self.interests.remove(&instance) {
            for ticket in interest.tickets {
                self.manager.unpin(ticket);
            }
        }
    }

    fn request(
        &mut self,
        instance: WorldInstanceId,
        key: SectorKey,
        manifest: WorldManifest,
    ) -> Option<SectorTicket> {
        let generation: GenerationRevision = manifest.generation().revision();
        let content: WorldContentRevision = manifest.content_revision();
        match self.manager.request(instance, key, generation, content) {
            Ok(ticket) => Some(ticket),
            Err(RequestError::CacheFull) if self.manager.evict_oldest() => self
                .manager
                .request(instance, key, generation, content)
                .ok(),
            Err(_) => None,
        }
    }
}

fn collision_from_sectors<'a>(sectors: impl Iterator<Item = &'a SectorGeometry>) -> CollisionIndex {
    let mut colliders = Vec::new();
    let mut seen_boxes = std::collections::HashSet::new();
    let mut triangles = Vec::new();
    let mut seen_triangles = std::collections::HashSet::new();
    for sector in sectors {
        for map_box in &sector.boxes {
            if map_box.solid && seen_boxes.insert(box_key(*map_box)) {
                colliders.push(Aabb::from_center_size(map_box.center, map_box.size));
            }
        }
        for triangle in &sector.triangles {
            if triangle.solid && seen_triangles.insert(triangle_key(*triangle)) {
                triangles.push(*triangle);
            }
        }
    }
    CollisionIndex::from_geometry(colliders, triangles)
}

fn box_key(map_box: MapBox) -> [u32; 6] {
    [
        map_box.center[0].to_bits(),
        map_box.center[1].to_bits(),
        map_box.center[2].to_bits(),
        map_box.size[0].to_bits(),
        map_box.size[1].to_bits(),
        map_box.size[2].to_bits(),
    ]
}

fn triangle_key(triangle: MapTriangle) -> [u32; 9] {
    let [a, b, c] = triangle.vertices;
    [
        a[0].to_bits(),
        a[1].to_bits(),
        a[2].to_bits(),
        b[0].to_bits(),
        b[1].to_bits(),
        b[2].to_bits(),
        c[0].to_bits(),
        c[1].to_bits(),
        c[2].to_bits(),
    ]
}
