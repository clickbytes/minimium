use super::sector_worker::{PreparedScene, SectorWorker};
use minimium_procgen::SectorGeometry;
use minimium_sectors::{InstallResult, RequestError, SectorManager, SectorTicket};
use minimium_simulation::CollisionIndex;
use minimium_world::world::{
    Aabb, GenerationRevision, MapBox, MapTriangle, SectorKey, SectorRevision, WorldContentRevision,
    WorldInstanceId, WorldManifest, WorldMap,
};
use std::{collections::HashMap, fmt, io, sync::Arc};

const CLIENT_SECTOR_CACHE_CAPACITY: usize = 512;
const MAX_CLIENT_INSTALLS_PER_FRAME: usize = 32;

#[derive(Debug)]
pub(super) enum SectorWorkerError {
    ThreadSpawn(io::Error),
    QueueFull,
    CacheFull,
    WorkerStopped(Vec<SectorTicket>),
}

impl SectorWorkerError {
    pub(super) const fn code(&self) -> &'static str {
        match self {
            Self::ThreadSpawn(_) => "client_runtime.sector_worker_start_failed",
            Self::QueueFull => "client_runtime.sector_worker_queue_full",
            Self::CacheFull => "client_runtime.sector_cache_full",
            Self::WorkerStopped(_) => "client_runtime.sector_worker_stopped",
        }
    }
}

impl fmt::Display for SectorWorkerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ThreadSpawn(error) => write!(formatter, "{}: {error}", self.code()),
            Self::QueueFull => formatter.write_str(self.code()),
            Self::CacheFull => formatter.write_str(self.code()),
            Self::WorkerStopped(tickets) => {
                write!(formatter, "{} ({} tickets)", self.code(), tickets.len())
            }
        }
    }
}

impl std::error::Error for SectorWorkerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ThreadSpawn(error) => Some(error),
            Self::QueueFull | Self::CacheFull | Self::WorkerStopped(_) => None,
        }
    }
}

pub(super) struct ClientSectorCache {
    manager: SectorManager<SectorGeometry>,
    collision_worker: super::collision_worker::CollisionWorker,
    scene_collision_worker: super::collision_worker::SceneCollisionWorker,
    next_collision_revision: u64,
    interests: HashMap<WorldInstanceId, SectorInterest>,
    workers: HashMap<WorldInstanceId, SectorWorker>,
    presentations: HashMap<WorldInstanceId, Arc<PreparedScene>>,
}

struct SectorInterest {
    keys: Vec<SectorKey>,
    tickets: Vec<SectorTicket>,
    generation: GenerationRevision,
    content_revision: WorldContentRevision,
    collision: Option<CollisionIndex>,
    collision_revision: u64,
    collision_pending: bool,
}

pub(super) struct ClientSectorScene {
    pub(super) prepared: Option<Arc<PreparedScene>>,
    pub(super) origin: [f32; 3],
    pub(super) origin_generation: u64,
    pub(super) sectors: Vec<(SectorKey, Arc<SectorGeometry>)>,
    pub(super) render_sectors: Vec<(SectorKey, u8, Arc<SectorGeometry>)>,
    pub(super) collision: CollisionIndex,
    pub(super) generation: GenerationRevision,
    pub(super) content_revision: WorldContentRevision,
    pub(super) collision_revision: u64,
}

impl ClientSectorCache {
    pub(super) fn shutdown_until(&mut self, deadline: std::time::Instant) -> Vec<&'static str> {
        let mut unfinished = Vec::new();
        for (_, mut worker) in self.workers.drain() {
            if !worker.shutdown_until(deadline) {
                unfinished.push("client-terrain-presentation");
            }
        }
        if !self.collision_worker.shutdown_until(deadline) {
            unfinished.push("client-sector-materialization");
        }
        if !self.scene_collision_worker.shutdown_until(deadline) {
            unfinished.push("client-collision-index");
        }
        unfinished
    }

    pub(super) fn presentation(&self, instance: WorldInstanceId) -> Option<&PreparedScene> {
        self.presentations.get(&instance).map(AsRef::as_ref)
    }
    pub(super) fn reset(&mut self) -> Result<(), SectorWorkerError> {
        *self = Self::new()?;
        Ok(())
    }
    pub(super) fn new() -> Result<Self, SectorWorkerError> {
        Ok(Self {
            manager: SectorManager::new(CLIENT_SECTOR_CACHE_CAPACITY, CLIENT_SECTOR_CACHE_CAPACITY)
                .expect("client sector cache capacity is nonzero"),
            collision_worker: super::collision_worker::CollisionWorker::new()?,
            scene_collision_worker: super::collision_worker::SceneCollisionWorker::new()?,
            next_collision_revision: 1,
            interests: HashMap::new(),
            workers: HashMap::new(),
            presentations: HashMap::new(),
        })
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
                && interest.tickets.iter().all(|ticket| {
                    self.manager
                        .state(*ticket)
                        .is_some_and(|state| state != minimium_sectors::SectorState::Failed)
                })
                && interest.generation == manifest.generation().revision()
                && interest.content_revision == manifest.content_revision()
        })
    }

    pub(super) fn ensure_scene(
        &mut self,
        instance: WorldInstanceId,
        manifest: WorldManifest,
        position: [f32; 3],
        view: minimium_renderer::terrain_view::TerrainView,
        origin: [f32; 3],
        origin_generation: u64,
        maps: &HashMap<WorldInstanceId, WorldMap>,
        manifests: &HashMap<WorldInstanceId, WorldManifest>,
        geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    ) -> Result<Option<ClientSectorScene>, SectorWorkerError> {
        if maps.get(&instance).is_none() || manifests.get(&instance) != Some(&manifest) {
            return Ok(None);
        }
        // Earth jobs must wait for the session's validated geography, including
        // bootstrap frames before Welcome. Never dispatch a job that would panic.
        if manifest.planet().is_some_and(|p| p.mode == 1) && geography.is_none() {
            return Ok(None);
        }
        let keys = minimium_procgen::sector_neighborhood_for(manifest, position);
        let same_interest = self.interest_matches(instance, &keys, manifest);
        if !same_interest {
            if let Some(previous) = self.interests.remove(&instance) {
                for ticket in previous.tickets {
                    self.manager.unpin(ticket);
                    let _ = self.manager.cancel(ticket);
                }
            }
            let mut tickets = Vec::with_capacity(keys.len());
            let mut request_error = None;
            for key in &keys {
                match self.request(instance, *key, manifest) {
                    Ok(Some(ticket)) if self.manager.pin(ticket) => tickets.push(ticket),
                    Ok(Some(ticket)) => {
                        let _ = self.manager.cancel(ticket);
                        request_error = Some(SectorWorkerError::CacheFull);
                        break;
                    }
                    Ok(None) => {
                        request_error = Some(SectorWorkerError::CacheFull);
                        break;
                    }
                    Err(error) => {
                        request_error = Some(error);
                        break;
                    }
                }
            }
            if let Some(error) = request_error {
                for ticket in tickets {
                    self.manager.unpin(ticket);
                    let _ = self.manager.cancel(ticket);
                }
                return Err(error);
            }
            let collision_revision = self.next_collision_revision;
            self.next_collision_revision = self.next_collision_revision.wrapping_add(1).max(1);
            self.interests.insert(
                instance,
                SectorInterest {
                    keys: keys.clone(),
                    tickets,
                    generation: manifest.generation().revision(),
                    content_revision: manifest.content_revision(),
                    collision: None,
                    collision_revision,
                    collision_pending: false,
                },
            );
        }

        let completed = self.collision_worker.poll().map_err(|error| {
            if let SectorWorkerError::WorkerStopped(tickets) = &error {
                for ticket in tickets {
                    if self.manager.state(*ticket)
                        == Some(minimium_sectors::SectorState::Generating)
                    {
                        let _ = self.manager.fail(*ticket);
                    } else {
                        let _ = self.manager.cancel(*ticket);
                    }
                }
            }
            error
        })?;
        for (ticket, geometry) in completed {
            let revision = SectorRevision::new(1).expect("initial revision");
            match self.manager.complete(ticket, revision, geometry) {
                InstallResult::Installed => {
                    let _ = self.manager.install_collision(ticket);
                }
                InstallResult::Stale => {}
                InstallResult::WrongState => {
                    let _ = self.manager.fail(ticket);
                }
            }
        }
        for _ in 0..MAX_CLIENT_INSTALLS_PER_FRAME {
            if !self.collision_worker.has_capacity() {
                break;
            }
            let Some(ticket) = self.manager.begin_next() else {
                break;
            };
            let Some(source_map) = maps.get(&ticket.instance()) else {
                let _ = self.manager.fail(ticket);
                continue;
            };
            let Some(source_manifest) = manifests.get(&ticket.instance()) else {
                let _ = self.manager.fail(ticket);
                continue;
            };
            if let Err(error) = self.collision_worker.submit(
                ticket,
                source_map.clone(),
                *source_manifest,
                geography.clone(),
            ) {
                let _ = self.manager.fail(ticket);
                return Err(error);
            }
            continue;
        }

        let Some(interest) = self.interests.get(&instance) else {
            return Ok(None);
        };
        if interest.keys != keys
            || interest.tickets.len() != keys.len()
            || !interest
                .tickets
                .iter()
                .all(|ticket| self.manager.collision_ready(*ticket))
        {
            return Ok(None);
        }
        let sectors: Option<Vec<_>> = interest
            .tickets
            .iter()
            .map(|ticket| Some((ticket.key(), self.manager.payload_arc(*ticket)?)))
            .collect::<Option<_>>();
        let Some(sectors) = sectors else {
            return Ok(None);
        };
        for result in self.scene_collision_worker.poll()? {
            if let Some(interest) = self.interests.get_mut(&result.instance) {
                if interest.collision_revision == result.revision {
                    interest.collision = Some((*result.index).clone());
                    interest.collision_pending = false;
                }
            }
        }
        let Some(interest) = self.interests.get_mut(&instance) else {
            return Ok(None);
        };
        if interest.collision.is_none() && !interest.collision_pending {
            let revision = interest.collision_revision;
            self.scene_collision_worker.submit(
                instance,
                revision,
                sectors
                    .iter()
                    .map(|(_, geometry)| geometry.clone())
                    .collect(),
            )?;
            interest.collision_pending = true;
            return Ok(None);
        }
        let Some(collision) = interest.collision.clone() else {
            return Ok(None);
        };
        let mut prepared = None;
        let render_sectors = if manifest.topology() == minimium_world::world::WorldTopology::Planet
        {
            if !self.workers.contains_key(&instance) {
                self.workers.insert(instance, SectorWorker::new()?);
            }
            let worker = self.workers.get(&instance).expect("worker inserted");
            if let Some(result) = worker.poll(
                instance,
                manifest,
                view,
                origin,
                origin_generation,
                &sectors,
                geography,
            )? {
                self.presentations.insert(instance, result);
            }
            if let Some(result) = self
                .presentations
                .get(&instance)
                .filter(|r| r.matches(instance, manifest, view, origin, origin_generation))
            {
                prepared = Some(result.clone());
                result
                    .patches
                    .iter()
                    .map(|p| (p.key, p.lod, p.geometry.clone()))
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            sectors.iter().map(|(k, g)| (*k, 0, g.clone())).collect()
        };
        Ok(Some(ClientSectorScene {
            prepared,
            origin,
            origin_generation,
            sectors,
            render_sectors,
            collision,
            generation: manifest.generation().revision(),
            content_revision: manifest.content_revision(),
            collision_revision: interest.collision_revision,
        }))
    }

    pub(super) fn release_interest(&mut self, instance: WorldInstanceId) {
        self.presentations.remove(&instance);
        self.workers.remove(&instance);
        if let Some(interest) = self.interests.remove(&instance) {
            for ticket in interest.tickets {
                self.manager.unpin(ticket);
                let _ = self.manager.cancel(ticket);
            }
        }
    }

    fn request(
        &mut self,
        instance: WorldInstanceId,
        key: SectorKey,
        manifest: WorldManifest,
    ) -> Result<Option<SectorTicket>, SectorWorkerError> {
        let generation: GenerationRevision = manifest.generation().revision();
        let content: WorldContentRevision = manifest.content_revision();
        match self.manager.request(instance, key, generation, content) {
            Ok(ticket) => Ok(Some(ticket)),
            Err(RequestError::CacheFull) if self.manager.evict_oldest() => self
                .manager
                .request(instance, key, generation, content)
                .map(Some)
                .map_err(|error| match error {
                    RequestError::QueueFull => SectorWorkerError::QueueFull,
                    _ => SectorWorkerError::CacheFull,
                }),
            Err(RequestError::QueueFull) => Err(SectorWorkerError::QueueFull),
            Err(_) => Err(SectorWorkerError::CacheFull),
        }
    }
}

pub(super) fn collision_from_sectors<'a>(
    sectors: impl Iterator<Item = &'a SectorGeometry>,
) -> CollisionIndex {
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
