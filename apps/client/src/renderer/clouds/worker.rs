use super::{cache::MeshCache, Publication, Request};
use glam::DVec3;
use minimium_environment::weather::{SeasonalProfile, TerrainSampler, WeatherModel};
use minimium_procgen::PlanetTerrain;
use std::sync::{Arc, Condvar, Mutex};

struct ClientTerrain {
    terrain: PlanetTerrain,
    radius_m: f64,
    relief_m: f64,
}

impl TerrainSampler for ClientTerrain {
    fn sample(&self, direction: DVec3) -> (f64, bool) {
        let point = self
            .terrain
            .sample(minimium_environment::weather::surface_key(direction));
        (point.height_mm as f64 / 1000.0 - self.radius_m, point.ocean)
    }

    fn contains_lake(&self, latitude_degrees: f64, longitude_degrees: f64) -> bool {
        crate::environment::lakes::contains(latitude_degrees, longitude_degrees)
    }

    fn earth_elevation_upper_bound_m(&self, direction: DVec3, angle: f64) -> Option<f64> {
        self.terrain
            .earth_elevation_upper_bound_m(direction.to_array(), angle)
            .ok()
    }

    fn relief_bound_m(&self) -> f64 {
        self.relief_m
    }
}

fn create_weather_model(
    manifest: minimium_world::world::WorldManifest,
    profile: SeasonalProfile,
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
) -> Option<WeatherModel<ClientTerrain>> {
    let surface = minimium_procgen::planet_surface_for(manifest);
    let mut terrain = PlanetTerrain::new(manifest.generation(), surface)?;
    if let Some(definition) = manifest.planet() {
        terrain = terrain.with_definition(definition);
    }
    if let Some(geography) = geography {
        terrain = terrain.with_geography(geography);
    } else if manifest.planet().is_some_and(|planet| planet.mode == 1) {
        return None;
    }
    let radius_m = manifest
        .planet()
        .map_or(surface.radius_mm(), |planet| planet.radius_mm) as f64
        / 1000.0;
    let is_earth = manifest.planet().is_some_and(|planet| planet.mode == 1);
    WeatherModel::new(
        manifest,
        profile,
        radius_m,
        is_earth,
        ClientTerrain {
            terrain,
            radius_m,
            relief_m: surface.relief_mm() as f64 / 1000.0,
        },
    )
}

#[derive(Default)]
struct Mailbox {
    pending: Option<Request>,
    completed: Option<Arc<Publication>>,
    stop: bool,
}

pub(super) struct CloudWorker {
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl CloudWorker {
    pub(super) fn new() -> Self {
        let mailbox = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let shared = Arc::clone(&mailbox);
        let thread = std::thread::Builder::new()
            .name("regional-cloud-mesh".into())
            .spawn(move || {
                let mut model: Option<WeatherModel<ClientTerrain>> = None;
                let mut meshes = MeshCache::default();
                let mut stamp = None;
                loop {
                    let request = {
                        let (lock, wake) = &*shared;
                        let mut state = lock.lock().unwrap();
                        while state.pending.is_none() && !state.stop {
                            state = wake.wait(state).unwrap();
                        }
                        if state.stop {
                            break;
                        }
                        state.pending.take().unwrap()
                    };
                    let started = std::time::Instant::now();
                    if stamp != Some(request.stamp) {
                        meshes = MeshCache::default();
                        stamp = Some(request.stamp);
                    }
                    if model
                        .as_ref()
                        .is_none_or(|m| m.manifest != request.stamp.manifest)
                    {
                        model = create_weather_model(
                            request.stamp.manifest,
                            SeasonalProfile::AnnualMean,
                            request.geography.clone(),
                        );
                    }
                    let Some(model) = &mut model else {
                        continue;
                    };
                    let snapshot = model.query(request.observer, request.utc, request.revision);
                    let mesh = meshes.build(&snapshot, request.detail, || {
                        let state = shared.0.lock().unwrap();
                        state.stop
                            || state.pending.as_ref().is_some_and(|next| {
                                next.stamp != request.stamp || next.detail != request.detail
                            })
                    });
                    let Some(mesh) = mesh else {
                        continue;
                    };
                    let mut publication = Publication {
                        request,
                        snapshot,
                        faces: mesh.faces,
                        morphs: mesh.morphs,
                        limited: mesh.stats.simplified > 0,
                        stats: mesh.stats,
                        build_ms: started.elapsed().as_secs_f64() * 1000.0,
                    };
                    if publication.request.sort {
                        sort(&mut publication, model.radius);
                    }
                    publication.build_ms = started.elapsed().as_secs_f64() * 1000.0;
                    let mut state = shared.0.lock().unwrap();
                    if state.stop {
                        break;
                    }
                    // A request for a different clock/world invalidates in-flight work.
                    if state
                        .pending
                        .as_ref()
                        .is_none_or(|next| next.stamp == publication.request.stamp)
                    {
                        state.completed = Some(Arc::new(publication));
                    }
                }
            })
            .expect("cloud worker thread");
        Self {
            mailbox,
            thread: Some(thread),
        }
    }

    pub(super) fn submit(&self, request: Request) {
        self.mailbox.0.lock().unwrap().pending = Some(request);
        self.mailbox.1.notify_one();
    }

    pub(super) fn take(&self) -> Option<Arc<Publication>> {
        self.mailbox.0.lock().unwrap().completed.take()
    }

    pub(super) fn clear(&self) {
        let mut state = self.mailbox.0.lock().unwrap();
        state.pending = None;
        state.completed = None;
    }
}

impl Drop for CloudWorker {
    fn drop(&mut self) {
        self.mailbox.0.lock().unwrap().stop = true;
        self.mailbox.1.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn sort(publication: &mut Publication, radius: f64) {
    let transforms: Vec<_> = publication
        .snapshot
        .clouds
        .iter()
        .map(|c| {
            let up = c.direction_at(publication.snapshot.utc);
            let east = glam::DVec3::new(-up.z, 0.0, up.x).normalize_or_zero();
            (up * (radius + c.base_m), up, east, up.cross(east))
        })
        .collect();
    publication.faces.sort_by_cached_key(|face| {
        let (center, up, east, north) = transforms[face.center[3] as usize];
        let p = center
            + east * f64::from(face.center[0])
            + up * f64::from(face.center[1])
            + north * f64::from(face.center[2]);
        std::cmp::Reverse((p - publication.request.eye).length_squared().to_bits())
    });
}
