//! Latest-request worker with bounded geometry/prepared caches; never owns GPU handles.
mod water;

use water::build_water_vertices;
pub(super) use water::WaterPresentation;

use super::sector_scene::{SceneInterestOwner, SectorWorkerError};
use minimium_procgen::{planet_surface_for, PlanetTerrain, SectorContentVersion, SectorGeometry};
use minimium_renderer::{prepared_sector::PreparedSector, render_patch_cache::RenderPatchCache};
use minimium_world::world::{SectorKey, WorldInstanceId, WorldManifest};
use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};
#[derive(Clone, Copy, PartialEq)]
struct Key {
    terrain_revision: SectorContentVersion,
    world: WorldInstanceId,
    manifest: WorldManifest,
    origin: [f32; 3],
    origin_generation: u64,
    water_level_revision: u64,
    fluid_revision: u64,
    view: minimium_renderer::terrain_view::TerrainView,
}
impl Key {
    fn same_geometry(self, other: Self) -> bool {
        self.world == other.world
            && self.manifest == other.manifest
            && self.terrain_revision == other.terrain_revision
            && self.origin == other.origin
            && self.origin_generation == other.origin_generation
            && self.view == other.view
    }
}
#[derive(Clone)]
struct Request {
    owner: SceneInterestOwner,
    key: Key,
    collision: Vec<(SectorKey, Arc<SectorGeometry>)>,
    geography: minimium_procgen::TerrainInputs,
    water: Option<WaterPresentation>,
}

pub(super) struct PreparedScene {
    key: Key,
    pub patches: Vec<Arc<PreparedSector>>,
    pub water_vertices: Arc<Vec<minimium_renderer::ocean_geometry::OceanVertex>>,
    pub build_ms: f64,
    pub cache_bytes: usize,
}
impl PreparedScene {
    pub fn terrain_revision(&self) -> SectorContentVersion {
        self.key.terrain_revision
    }
    pub fn matches(
        &self,
        world: WorldInstanceId,
        manifest: WorldManifest,
        view: minimium_renderer::terrain_view::TerrainView,
        origin: [f32; 3],
        origin_generation: u64,
        water_level_revision: u64,
        fluid_revision: u64,
    ) -> bool {
        self.key.world == world
            && self.key.manifest == manifest
            && self.key.view == view
            && self.key.origin == origin
            && self.key.origin_generation == origin_generation
            && self.key.water_level_revision == water_level_revision
            && self.key.fluid_revision == fluid_revision
    }
    pub fn geometry_matches(
        &self,
        world: WorldInstanceId,
        manifest: WorldManifest,
        origin: [f32; 3],
        origin_generation: u64,
        terrain_revision: SectorContentVersion,
    ) -> bool {
        self.key.world == world
            && self.key.manifest == manifest
            && self.key.origin == origin
            && self.key.origin_generation == origin_generation
            && self.key.terrain_revision == terrain_revision
    }
    pub fn position(&self) -> [f32; 3] {
        self.key.view.position
    }
}
#[derive(Default)]
struct State {
    pending: HashMap<SceneInterestOwner, Request>,
    pending_order: std::collections::VecDeque<SceneInterestOwner>,
    requested: HashMap<SceneInterestOwner, Key>,
    completed: HashMap<SceneInterestOwner, Arc<PreparedScene>>,
    failed: HashMap<SceneInterestOwner, (Key, minimium_procgen::ExcavationGeometryError)>,
    stop: bool,
}
pub(super) struct SectorWorker {
    state: Arc<(Mutex<State>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}
impl SectorWorker {
    pub(super) fn shutdown_until(&mut self, deadline: std::time::Instant) -> bool {
        self.state.0.lock().unwrap().stop = true;
        self.state.1.notify_one();
        while self
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        match self.thread.take() {
            Some(thread) if thread.is_finished() => thread.join().is_ok(),
            Some(_) => false,
            None => true,
        }
    }

    pub fn new() -> Result<Self, SectorWorkerError> {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let shared = state.clone();
        let thread = std::thread::Builder::new()
            .name("terrain-presentation".into())
            .spawn(move || {
                let mut geometry = RenderPatchCache::default();
                let mut edges = minimium_renderer::terrain_edges::EdgeConformance::default();
                let mut scope = None;
                let mut origin = [f32::INFINITY; 3];
                let mut prepared: HashMap<(SectorKey, u8), Arc<PreparedSector>> = HashMap::new();
                let mut prepared_water_revision = None;
                let mut water_mesh_cache: Option<(
                    WorldInstanceId,
                    WorldManifest,
                    u64,
                    u64,
                    [f32; 3],
                    Arc<Vec<minimium_renderer::ocean_geometry::OceanVertex>>,
                )> = None;
                loop {
                    let request = {
                        let mut s = shared.0.lock().unwrap();
                        while s.pending.is_empty() && !s.stop {
                            s = shared.1.wait(s).unwrap();
                        }
                        if s.stop {
                            break;
                        }
                        let owner = s.pending_order.pop_front().expect("pending scene request");
                        s.pending
                            .get(&owner)
                            .cloned()
                            .expect("queued scene request")
                    };
                    let started = std::time::Instant::now();
                    let water_changed =
                        prepared_water_revision != Some(request.key.water_level_revision);
                    let water_vertices = match (request.water.as_ref(), water_mesh_cache.as_ref()) {
                        (
                            Some(water),
                            Some((world, manifest, level_revision, revision, origin, mesh)),
                        ) if *world == request.key.world
                            && *manifest == request.key.manifest
                            && *level_revision == water.level_revision
                            && *revision == water.fluid_revision
                            && *origin == request.key.origin =>
                        {
                            Arc::clone(mesh)
                        }
                        (water, _) => {
                            let mesh = Arc::new(build_water_vertices(water, request.key.origin));
                            if let Some(water) = water {
                                water_mesh_cache = Some((
                                    request.key.world,
                                    request.key.manifest,
                                    water.level_revision,
                                    water.fluid_revision,
                                    request.key.origin,
                                    Arc::clone(&mesh),
                                ));
                            } else {
                                water_mesh_cache = None;
                            }
                            mesh
                        }
                    };
                    let next_scope = (request.key.world, request.key.manifest);
                    if scope != Some(next_scope) {
                        geometry = RenderPatchCache::default();
                        edges = Default::default();
                        prepared.clear();
                        prepared_water_revision = None;
                        scope = Some(next_scope);
                    }
                    if origin != request.key.origin {
                        prepared.clear();
                        prepared_water_revision = None;
                        origin = request.key.origin;
                    }
                    let manifest = request.key.manifest;
                    let mut terrain =
                        PlanetTerrain::new(manifest.generation(), planet_surface_for(manifest));
                    if let Some(t) = terrain.take() {
                        let t = if let Some(d) = manifest.planet() {
                            t.with_definition(d)
                        } else {
                            t
                        };
                        let t = t
                            .with_surface_recipe(minimium_game_sample::SURFACE_RECIPE)
                            .expect("validated sample surface recipe");
                        terrain = Some(t.with_inputs(request.geography.clone()));
                    }
                    let meshes =
                        if manifest.topology() == minimium_world::world::WorldTopology::Planet {
                            minimium_renderer::planet_visibility::render_patches_for_view(
                                request.key.world,
                                manifest,
                                request.key.view,
                                &request.collision,
                                request.geography.clone(),
                                &mut geometry,
                                &|| {
                                    let s = shared.0.lock().unwrap();
                                    s.stop
                                        || s.requested
                                            .get(&request.owner)
                                            .is_none_or(|key| !key.same_geometry(request.key))
                                },
                            )
                        } else {
                            Ok(request
                                .collision
                                .iter()
                                .map(|(k, g)| (*k, 0, g.clone()))
                                .collect())
                        };
                    let meshes = match meshes {
                        Ok(meshes) => meshes,
                        Err(error) => {
                            let mut state = shared.0.lock().unwrap();
                            if state.requested.get(&request.owner) == Some(&request.key) {
                                state.failed.insert(request.owner, (request.key, error));
                                state.pending.remove(&request.owner);
                            }
                            continue;
                        }
                    };
                    let meshes = if let Some(terrain) = terrain.as_ref() {
                        edges.prepare(meshes, terrain)
                    } else {
                        meshes
                    };
                    let required: std::collections::HashSet<_> =
                        meshes.iter().map(|(k, l, _)| (*k, *l)).collect();
                    prepared.retain(|k, _| required.contains(k));
                    let mut patches = Vec::with_capacity(meshes.len());
                    let mut cancelled = false;
                    for (key, lod, mesh) in meshes {
                        {
                            let s = shared.0.lock().unwrap();
                            if s.stop
                                || s.pending
                                    .get(&request.owner)
                                    .is_some_and(|r| !r.key.same_geometry(request.key))
                            {
                                cancelled = true;
                                break;
                            }
                        }
                        if prepared
                            .get(&(key, lod))
                            .is_some_and(|p| !Arc::ptr_eq(&p.geometry, &mesh))
                        {
                            prepared.remove(&(key, lod));
                        }
                        let new_patch = !prepared.contains_key(&(key, lod));
                        let item = prepared.entry((key, lod)).or_insert_with(|| {
                            Arc::new(PreparedSector::new_with_clipped_ocean(
                                key,
                                lod,
                                mesh,
                                if manifest.topology()
                                    == minimium_world::world::WorldTopology::Planet
                                {
                                    terrain.as_ref()
                                } else {
                                    None
                                },
                                edges.ocean(key, lod),
                                request.key.origin,
                                request.water.as_ref().map(|water| {
                                    (
                                        &*water.catalog,
                                        &*water.snapshot,
                                        water.interface_boxes.as_slice(),
                                    )
                                }),
                            ))
                        });
                        if water_changed || new_patch {
                            if let Some(ocean) = edges.ocean(key, lod) {
                                Arc::make_mut(item).ocean = Arc::new(
                                    minimium_renderer::prepared_sector::clipped_ocean_vertices(
                                        ocean,
                                        request.key.origin,
                                        request.water.as_ref().map(|water| {
                                            (
                                                &*water.catalog,
                                                &*water.snapshot,
                                                water.interface_boxes.as_slice(),
                                            )
                                        }),
                                    ),
                                );
                            }
                        }
                        patches.push(item.clone());
                    }
                    if cancelled {
                        continue;
                    }
                    let mut s = shared.0.lock().unwrap();
                    if !s.stop
                        && s.requested.get(&request.owner) == Some(&request.key)
                        && s.pending
                            .get(&request.owner)
                            .is_some_and(|pending| pending.key == request.key)
                    {
                        s.pending.remove(&request.owner);
                        s.completed.insert(
                            request.owner,
                            Arc::new(PreparedScene {
                                key: request.key,
                                patches,
                                water_vertices,
                                build_ms: started.elapsed().as_secs_f64() * 1000.0,
                                cache_bytes: geometry.bytes(),
                            }),
                        );
                        prepared_water_revision = Some(request.key.water_level_revision);
                    }
                }
            })
            .map_err(SectorWorkerError::ThreadSpawn)?;
        Ok(Self {
            state,
            thread: Some(thread),
        })
    }
    pub fn poll(
        &self,
        owner: SceneInterestOwner,
        world: WorldInstanceId,
        manifest: WorldManifest,
        view: minimium_renderer::terrain_view::TerrainView,
        origin: [f32; 3],
        origin_generation: u64,
        water: Option<WaterPresentation>,
        collision: &[(SectorKey, Arc<SectorGeometry>)],
        geography: minimium_procgen::TerrainInputs,
    ) -> Result<Option<Arc<PreparedScene>>, SectorWorkerError> {
        let key = Key {
            terrain_revision: geography.revision(),
            world,
            manifest,
            view,
            origin,
            origin_generation,
            water_level_revision: water.as_ref().map_or(0, |water| water.level_revision),
            fluid_revision: water.as_ref().map_or(0, |water| water.fluid_revision),
        };
        let mut s = self.state.0.lock().unwrap();
        if s.stop || self.thread.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(SectorWorkerError::WorkerStopped(Vec::new()));
        }
        // A newer wave must not repeatedly cancel a complete, still-current
        // terrain scene. Publish this water frame, then prepare the latest one.
        let completed = s
            .completed
            .remove(&owner)
            .filter(|scene| scene.key.same_geometry(key));
        if let Some((failed_key, error)) = s.failed.remove(&owner) {
            if failed_key == key {
                return Err(SectorWorkerError::Geometry(error));
            }
        }
        if s.requested.get(&owner) != Some(&key) {
            if s.requested.len() >= 3 && !s.requested.contains_key(&owner) {
                return Err(SectorWorkerError::QueueFull);
            }
            s.pending.insert(
                owner,
                Request {
                    owner,
                    key,
                    collision: collision.to_vec(),
                    geography,
                    water,
                },
            );
            if !s.pending_order.contains(&owner) {
                s.pending_order.push_back(owner);
            }
            s.requested.insert(owner, key);
            self.state.1.notify_one();
        }
        Ok(completed)
    }

    pub(super) fn cancel(&self, owner: SceneInterestOwner) -> bool {
        let mut state = self.state.0.lock().unwrap();
        state.pending.remove(&owner);
        state.pending_order.retain(|queued| *queued != owner);
        let requested = state.requested.remove(&owner).is_some();
        state.completed.remove(&owner);
        state.failed.remove(&owner);
        requested
    }

    pub(super) fn promote(&mut self, source: SceneInterestOwner, target: SceneInterestOwner) {
        let mut state = self.state.0.lock().unwrap();
        if let Some(key) = state.requested.remove(&source) {
            state.requested.insert(target, key);
        }
        if let Some(request) = state.pending.remove(&source) {
            let was_queued = state.pending_order.contains(&source);
            state.pending_order.retain(|owner| *owner != source);
            state.pending.insert(
                target,
                Request {
                    owner: target,
                    ..request
                },
            );
            if was_queued || !state.pending_order.contains(&target) {
                state.pending_order.push_back(target);
            }
            self.state.1.notify_one();
        }
        if let Some(scene) = state.completed.remove(&source) {
            state.completed.insert(target, scene);
        }
        if let Some((key, error)) = state.failed.remove(&source) {
            state.failed.insert(target, (key, error));
        }
    }
}
impl Drop for SectorWorker {
    fn drop(&mut self) {
        {
            self.state.0.lock().unwrap().stop = true;
            self.state.1.notify_one();
        }
        self.thread.take();
    }
}
