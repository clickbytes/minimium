//! CPU world geometry and collision readiness, independent of presentation.
mod collision;
mod error;
mod interests;
mod snapshot;
mod workers;
pub use error::WorldReadinessError;
use minimium_procgen::SectorGeometry;
use minimium_sectors::{InstallResult, RequestError, SectorManager, SectorTicket};
use minimium_simulation::CollisionIndex;
use minimium_world::world::{
    GenerationRevision, SectorKey, SectorRevision, WorldContentRevision, WorldInstanceId,
    WorldManifest, WorldMap,
};
pub use snapshot::{CollisionSnapshot, WorldReadinessOwner};
use std::{collections::HashMap, sync::Arc};
use WorldReadinessOwner as SceneInterestOwner;

const CLIENT_SECTOR_CACHE_CAPACITY: usize = 512;
const MAX_CLIENT_INSTALLS_PER_FRAME: usize = 32;

pub struct WorldReadiness {
    terrain_inputs: HashMap<WorldInstanceId, Arc<minimium_procgen::TerrainInputs>>,
    world_sources: HashMap<WorldInstanceId, (Arc<WorldMap>, WorldManifest)>,
    manager: SectorManager<SectorGeometry>,
    collision_worker: workers::CollisionWorker,
    scene_collision_worker: workers::SceneCollisionWorker,
    next_collision_revision: u64,
    interests: HashMap<SceneInterestOwner, SectorInterest>,
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

impl WorldReadiness {
    pub fn new() -> Result<Self, WorldReadinessError> {
        Ok(Self {
            terrain_inputs: HashMap::new(),
            world_sources: HashMap::new(),
            manager: SectorManager::new(CLIENT_SECTOR_CACHE_CAPACITY, CLIENT_SECTOR_CACHE_CAPACITY)
                .expect("nonzero cache capacity"),
            collision_worker: workers::CollisionWorker::new()?,
            scene_collision_worker: workers::SceneCollisionWorker::new()?,
            next_collision_revision: 1,
            interests: HashMap::new(),
        })
    }

    pub fn shutdown_until(&mut self, deadline: std::time::Instant) -> Vec<&'static str> {
        let mut unfinished = Vec::new();
        if !self.collision_worker.shutdown_until(deadline) {
            unfinished.push("client-sector-materialization");
        }
        if !self.scene_collision_worker.shutdown_until(deadline) {
            unfinished.push("client-collision-index");
        }
        unfinished
    }

    /// Requests the existing simulation-sector neighborhood, with no camera or GPU inputs.
    pub fn ensure_collision(
        &mut self,
        owner: WorldReadinessOwner,
        manifest: WorldManifest,
        position: [f32; 3],
        map: &WorldMap,
        geography: minimium_procgen::TerrainInputs,
    ) -> Result<Option<CollisionSnapshot>, WorldReadinessError> {
        self.ensure_collision_for_step(owner, manifest, position, position, map, geography)
    }

    /// Covers both ends of one speculative fixed step using the unchanged
    /// simulation neighborhoods. It does not certify an arbitrary long sweep.
    pub fn ensure_collision_for_step(
        &mut self,
        owner: WorldReadinessOwner,
        manifest: WorldManifest,
        previous_position: [f32; 3],
        position: [f32; 3],
        map: &WorldMap,
        geography: minimium_procgen::TerrainInputs,
    ) -> Result<Option<CollisionSnapshot>, WorldReadinessError> {
        let instance = owner.instance();
        if !snapshot::valid_position(manifest, position)
            || !snapshot::valid_position(manifest, previous_position)
            || !owner.matches_manifest(manifest)
            || !Self::content_profiles_match(&geography, instance, manifest, Some(map))
        {
            return Err(WorldReadinessError::InvalidRequest);
        }
        if (!self.interests.contains_key(&owner) && self.interests.len() >= 3)
            || (!self.world_sources.contains_key(&instance)
                && self.world_sources.len() >= CLIENT_SECTOR_CACHE_CAPACITY)
        {
            return Err(WorldReadinessError::CacheFull);
        }
        self.update_terrain_input(instance, manifest, &geography);
        self.cache_world_source(instance, map, manifest);
        let mut keys = minimium_procgen::sector_neighborhood_for(manifest, position);
        for key in minimium_procgen::sector_neighborhood_for(manifest, previous_position) {
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        let same_interest = self.interest_matches(owner, &keys, manifest);
        if !same_interest {
            let next_revision = self
                .next_collision_revision
                .checked_add(1)
                .ok_or(WorldReadinessError::RevisionExhausted)?;
            self.release_interest(owner);
            let mut tickets = Vec::with_capacity(keys.len());
            let mut request_error = None;
            for key in &keys {
                match self.request(instance, *key, manifest) {
                    Ok(Some(ticket)) if self.manager.pin(ticket) => tickets.push(ticket),
                    Ok(Some(ticket)) => {
                        let _ = self.manager.cancel(ticket);
                        request_error = Some(WorldReadinessError::CacheFull);
                        break;
                    }
                    Ok(None) => {
                        request_error = Some(WorldReadinessError::CacheFull);
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
                    let shared = self
                        .interests
                        .values()
                        .any(|other| other.tickets.contains(&ticket));
                    if !shared {
                        let _ = self.manager.cancel(ticket);
                    }
                }
                return Err(error);
            }
            let collision_revision = self.next_collision_revision;
            self.next_collision_revision = next_revision;
            self.interests.insert(
                owner,
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
            if let WorldReadinessError::WorkerStopped(tickets) = &error {
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
            // Cancelled/replaced work cannot fail or publish into a newer interest.
            if self.manager.state(ticket) != Some(minimium_sectors::SectorState::Generating) {
                continue;
            }
            let geometry = geometry.map_err(|error| {
                self.manager.fail(ticket);
                WorldReadinessError::Geometry(error)
            })?;
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
            let ticket_instance = ticket.instance();
            let Some((source_map, source_manifest)) = self.world_sources.get(&ticket_instance)
            else {
                let _ = self.manager.fail(ticket);
                continue;
            };
            let Some(source_geography) = self.terrain_inputs.get(&ticket_instance) else {
                let _ = self.manager.fail(ticket);
                continue;
            };
            if let Err(error) = self.collision_worker.submit(
                ticket,
                source_map.as_ref().clone(),
                *source_manifest,
                source_geography.as_ref().clone(),
            ) {
                let _ = self.manager.fail(ticket);
                return Err(error);
            }
            continue;
        }

        let Some(interest) = self.interests.get(&owner) else {
            return Ok(None);
        };
        if !keys.iter().all(|key| interest.keys.contains(key))
            || interest.tickets.len() != interest.keys.len()
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
            if let Some(interest) = self.interests.get_mut(&result.owner) {
                if interest.collision_revision == result.revision {
                    interest.collision = Some((*result.index).clone());
                    interest.collision_pending = false;
                }
            }
        }
        let Some(interest) = self.interests.get_mut(&owner) else {
            return Ok(None);
        };
        if interest.collision.is_none() && !interest.collision_pending {
            let revision = interest.collision_revision;
            self.scene_collision_worker.submit(
                owner,
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
        Ok(Some(CollisionSnapshot {
            owner,
            manifest,
            map_fingerprint: map.fingerprint(),
            terrain_revision: geography.revision(),
            collision_revision: interest.collision_revision,
            position,
            sectors: Arc::from(sectors),
            collision,
            terrain_inputs: geography,
        }))
    }

    pub fn remove_world_data(&mut self, instance: WorldInstanceId) {
        self.release_world_interests(instance);
        self.terrain_inputs.remove(&instance);
        self.world_sources.remove(&instance);
    }

    fn request(
        &mut self,
        instance: WorldInstanceId,
        key: SectorKey,
        manifest: WorldManifest,
    ) -> Result<Option<SectorTicket>, WorldReadinessError> {
        let generation: GenerationRevision = manifest.generation().revision();
        let content: WorldContentRevision = manifest.content_revision();
        match self.manager.request(instance, key, generation, content) {
            Ok(ticket) => Ok(Some(ticket)),
            Err(RequestError::CacheFull) if self.manager.evict_oldest() => self
                .manager
                .request(instance, key, generation, content)
                .map(Some)
                .map_err(|error| match error {
                    RequestError::QueueFull => WorldReadinessError::QueueFull,
                    _ => WorldReadinessError::CacheFull,
                }),
            Err(RequestError::QueueFull) => Err(WorldReadinessError::QueueFull),
            Err(_) => Err(WorldReadinessError::CacheFull),
        }
    }

    fn cache_world_source(
        &mut self,
        instance: WorldInstanceId,
        map: &WorldMap,
        manifest: WorldManifest,
    ) {
        let fingerprint = map.fingerprint();
        let unchanged =
            self.world_sources
                .get(&instance)
                .is_some_and(|(current, current_manifest)| {
                    *current_manifest == manifest && current.fingerprint() == fingerprint
                });
        if !unchanged {
            self.manager.invalidate_where(instance, |_| true);
            self.world_sources
                .insert(instance, (Arc::new(map.clone()), manifest));
        }
    }
}
