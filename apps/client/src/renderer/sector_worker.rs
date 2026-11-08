//! Latest-request worker with bounded geometry/prepared caches; never owns GPU handles.
use super::sector_scene::SectorWorkerError;
use minimium_procgen::{PlanetTerrain, PreparedGeography, SectorGeometry, planet_surface_for};
use minimium_renderer::{prepared_sector::PreparedSector, render_patch_cache::RenderPatchCache};
use minimium_world::world::{SectorKey, WorldInstanceId, WorldManifest};
use std::{
    collections::HashMap,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};
#[derive(Clone, Copy, PartialEq)]
struct Key {
    world: WorldInstanceId,
    manifest: WorldManifest,
    origin: [f32; 3],
    origin_generation: u64,
    view: minimium_renderer::terrain_view::TerrainView,
}
struct Request {
    key: Key,
    collision: Vec<(SectorKey, Arc<SectorGeometry>)>,
    geography: Option<Arc<dyn PreparedGeography>>,
}
pub(super) struct PreparedScene {
    key: Key,
    pub patches: Vec<Arc<PreparedSector>>,
    pub build_ms: f64,
    pub cache_bytes: usize,
}
impl PreparedScene {
    pub fn matches(
        &self,
        world: WorldInstanceId,
        manifest: WorldManifest,
        view: minimium_renderer::terrain_view::TerrainView,
        origin: [f32; 3],
        origin_generation: u64,
    ) -> bool {
        self.key.world == world
            && self.key.manifest == manifest
            && self.key.view == view
            && self.key.origin == origin
            && self.key.origin_generation == origin_generation
    }
    pub fn position(&self) -> [f32; 3] {
        self.key.view.position
    }
}
#[derive(Default)]
struct State {
    pending: Option<Request>,
    requested: Option<Key>,
    completed: Option<Arc<PreparedScene>>,
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
                loop {
                    let request = {
                        let mut s = shared.0.lock().unwrap();
                        while s.pending.is_none() && !s.stop {
                            s = shared.1.wait(s).unwrap();
                        }
                        if s.stop {
                            break;
                        }
                        s.pending.take().unwrap()
                    };
                    let started = std::time::Instant::now();
                    let next_scope = (request.key.world, request.key.manifest);
                    if scope != Some(next_scope) {
                        geometry = RenderPatchCache::default();
                        edges = Default::default();
                        prepared.clear();
                        scope = Some(next_scope);
                    }
                    if origin != request.key.origin {
                        prepared.clear();
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
                        terrain = Some(if let Some(g) = request.geography.clone() {
                            t.with_geography(g)
                        } else {
                            t
                        });
                    }
                    let meshes = if manifest.topology()
                        == minimium_world::world::WorldTopology::Planet
                    {
                        minimium_renderer::planet_visibility::render_patches_for_view(
                            manifest,
                            request.key.view,
                            &request.collision,
                            request.geography.clone(),
                            &mut geometry,
                            &|| {
                                let s = shared.0.lock().unwrap();
                                s.stop || s.pending.as_ref().is_some_and(|r| r.key != request.key)
                            },
                        )
                    } else {
                        request
                            .collision
                            .iter()
                            .map(|(k, g)| (*k, 0, g.clone()))
                            .collect()
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
                            if s.stop || s.pending.as_ref().is_some_and(|r| r.key != request.key) {
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
                        let item = prepared.entry((key, lod)).or_insert_with(|| {
                            Arc::new(PreparedSector::new(
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
                            ))
                        });
                        patches.push(item.clone());
                    }
                    if cancelled {
                        continue;
                    }
                    let mut s = shared.0.lock().unwrap();
                    if !s.stop && s.pending.as_ref().is_none_or(|r| r.key == request.key) {
                        s.completed = Some(Arc::new(PreparedScene {
                            key: request.key,
                            patches,
                            build_ms: started.elapsed().as_secs_f64() * 1000.0,
                            cache_bytes: geometry.bytes(),
                        }));
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
        world: WorldInstanceId,
        manifest: WorldManifest,
        view: minimium_renderer::terrain_view::TerrainView,
        origin: [f32; 3],
        origin_generation: u64,
        collision: &[(SectorKey, Arc<SectorGeometry>)],
        geography: Option<Arc<dyn PreparedGeography>>,
    ) -> Result<Option<Arc<PreparedScene>>, SectorWorkerError> {
        let key = Key {
            world,
            manifest,
            view,
            origin,
            origin_generation,
        };
        let mut s = self.state.0.lock().unwrap();
        if s.stop || self.thread.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(SectorWorkerError::WorkerStopped(Vec::new()));
        }
        let completed = s.completed.take().filter(|r| r.key == key);
        if s.requested != Some(key) {
            s.pending = Some(Request {
                key,
                collision: collision.to_vec(),
                geography,
            });
            s.requested = Some(key);
            self.state.1.notify_one();
        }
        Ok(completed)
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
