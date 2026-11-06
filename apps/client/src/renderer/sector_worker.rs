//! Latest-request worker with bounded geometry/prepared caches; never owns GPU handles.
use minimium_procgen::{planet_surface_for, PlanetTerrain, PreparedGeography, SectorGeometry};
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
        origin: [f32; 3],
    ) -> bool {
        self.key.world == world && self.key.manifest == manifest && self.key.origin == origin
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
    pub fn new() -> Self {
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
                    let meshes =
                        if manifest.topology() == minimium_world::world::WorldTopology::Planet {
                            minimium_renderer::planet_visibility::render_patches_for_view(
                                manifest,
                                request.key.view,
                                &request.collision,
                                request.geography.clone(),
                                &mut geometry,
                                &|| {
                                    let s = shared.0.lock().unwrap();
                                    s.stop
                                        || s.pending.as_ref().is_some_and(|r| {
                                            r.key.manifest != manifest
                                                || r.key.origin != request.key.origin
                                        })
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
                            if s.stop
                                || s.pending.as_ref().is_some_and(|r| {
                                    r.key.world != request.key.world
                                        || r.key.manifest != manifest
                                        || r.key.origin != request.key.origin
                                })
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
                    if !s.stop
                        && s.pending.as_ref().is_none_or(|r| {
                            r.key.world == request.key.world
                                && r.key.manifest == manifest
                                && r.key.origin == request.key.origin
                        })
                    {
                        s.completed = Some(Arc::new(PreparedScene {
                            key: request.key,
                            patches,
                            build_ms: started.elapsed().as_secs_f64() * 1000.0,
                            cache_bytes: geometry.bytes(),
                        }));
                    }
                }
            })
            .expect("terrain presentation worker");
        Self {
            state,
            thread: Some(thread),
        }
    }
    pub fn poll(
        &self,
        world: WorldInstanceId,
        manifest: WorldManifest,
        view: minimium_renderer::terrain_view::TerrainView,
        origin: [f32; 3],
        collision: &[(SectorKey, Arc<SectorGeometry>)],
        geography: Option<Arc<dyn PreparedGeography>>,
    ) -> Option<Arc<PreparedScene>> {
        let key = Key {
            world,
            manifest,
            view,
            origin,
        };
        let mut s = self.state.0.lock().unwrap();
        let completed = s.completed.take().filter(|r| {
            r.key.world == world && r.key.manifest == manifest && r.key.origin == origin
        });
        if s.requested != Some(key) {
            s.pending = Some(Request {
                key,
                collision: collision.to_vec(),
                geography,
            });
            s.requested = Some(key);
            self.state.1.notify_one();
        }
        completed
    }
}
impl Drop for SectorWorker {
    fn drop(&mut self) {
        {
            self.state.0.lock().unwrap().stop = true;
            self.state.1.notify_one();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
