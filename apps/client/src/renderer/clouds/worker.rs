use super::{cache::MeshCache, Publication, Request};
use glam::DVec3;
use minimium_environment::weather::{SeasonalProfile, TerrainSampler, WeatherModel};
use minimium_procgen::{GenerationError, PlanetTerrain};
use std::sync::{Arc, Condvar, Mutex};

struct ClientTerrain {
    terrain: PlanetTerrain,
    radius_m: f64,
    relief_m: f64,
}

impl TerrainSampler for ClientTerrain {
    type Error = GenerationError;

    fn source_elevation_m(&self, direction: DVec3) -> Result<Option<f64>, Self::Error> {
        self.terrain.source_elevation_m(direction.to_array())
    }

    fn sample(&self, direction: DVec3) -> Result<(f64, bool), Self::Error> {
        let point = self
            .terrain
            .sample(minimium_environment::weather::surface_key(direction))?;
        Ok((
            point.height_mm as f64 / 1000.0
                - self.radius_m
                - self.terrain.sea_level_mm() as f64 / 1000.0,
            point.ocean,
        ))
    }

    fn contains_lake(&self, latitude_degrees: f64, longitude_degrees: f64) -> bool {
        crate::environment::lakes::contains(latitude_degrees, longitude_degrees)
    }

    fn earth_elevation_upper_bound_m(
        &self,
        direction: DVec3,
        angle: f64,
    ) -> Result<Option<f64>, Self::Error> {
        if !self.terrain.is_earth() {
            return Ok(None);
        }
        self.terrain
            .earth_elevation_upper_bound_m(direction.to_array(), angle)
            .map(Some)
    }

    fn relief_bound_m(&self) -> f64 {
        self.relief_m
    }
}

fn create_weather_model(
    manifest: minimium_world::world::WorldManifest,
    profile: SeasonalProfile,
    terrain_inputs: minimium_procgen::TerrainInputs,
) -> Result<WeatherModel<ClientTerrain>, GenerationError> {
    let surface = minimium_procgen::planet_surface_for(manifest);
    let terrain = PlanetTerrain::for_manifest(manifest, terrain_inputs)?;
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
    .ok_or_else(|| GenerationError::invalid_manifest("weather model requires a valid planet"))
}

#[derive(Default)]
struct Mailbox {
    cover: Option<(
        super::Stamp,
        Arc<minimium_renderer::surface_cover::CoverGrid>,
    )>,
    pending: Option<Request>,
    completed: Option<Arc<Publication>>,
    failure: Option<(super::Stamp, GenerationError)>,
    stop: bool,
}

pub(super) struct CloudWorker {
    mailbox: Arc<(Mutex<Mailbox>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl CloudWorker {
    pub(super) fn shutdown_until(&mut self, deadline: std::time::Instant) -> bool {
        self.mailbox.0.lock().unwrap().stop = true;
        self.mailbox.1.notify_one();
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

    pub(super) fn new() -> Self {
        let mailbox = Arc::new((Mutex::new(Mailbox::default()), Condvar::new()));
        let shared = Arc::clone(&mailbox);
        let thread = std::thread::Builder::new()
            .name("regional-cloud-mesh".into())
            .spawn(move || {
                let mut model: Option<WeatherModel<ClientTerrain>> = None;
                let mut meshes = MeshCache::default();
                let mut stamp = None;
                let mut cover_stamp = None;
                let mut atlas = None;
                let mut atlas_stamp = None;
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
                        model = match create_weather_model(
                            request.stamp.manifest,
                            SeasonalProfile::Seasonal,
                            request.terrain_inputs.clone(),
                        ) {
                            Ok(model) => Some(model),
                            Err(error) => {
                                fail(&shared, request.stamp, error);
                                continue;
                            }
                        };
                        stamp = Some(request.stamp);
                    }
                    let Some(model) = &mut model else {
                        continue;
                    };
                    if model.set_scenario((*request.climate).clone()).is_err() {
                        continue;
                    }
                    let next_cover_stamp = (
                        request.stamp,
                        (request.utc / 3600.0).floor() as i64,
                        minimium_renderer::surface_cover::CoverAddress::at(request.observer),
                    );
                    let next_atlas_stamp = (request.stamp, next_cover_stamp.1);
                    if atlas_stamp.as_ref() != Some(&next_atlas_stamp) {
                        let built = minimium_renderer::surface_atlas::SurfaceAtlas::build(
                            model,
                            request.utc,
                            || {
                                let state = shared.0.lock().unwrap();
                                state.stop
                                    || state
                                        .pending
                                        .as_ref()
                                        .is_some_and(|next| next.stamp != request.stamp)
                            },
                        );
                        let built = match built {
                            Ok(Some(built)) => built,
                            Ok(None) => continue,
                            Err(error) => {
                                fail(&shared, request.stamp, error);
                                continue;
                            }
                        };
                        atlas = Some(Arc::new(built));
                        atlas_stamp = Some(next_atlas_stamp);
                        cover_stamp = None;
                    }
                    if cover_stamp
                        .as_ref()
                        .is_none_or(|(key, _)| *key != next_cover_stamp)
                    {
                        let mut cover = match minimium_renderer::surface_cover::CoverGrid::build(
                            model,
                            request.observer,
                            request.utc,
                        ) {
                            Ok(cover) => cover,
                            Err(error) => {
                                fail(&shared, request.stamp, error);
                                continue;
                            }
                        };
                        cover.atlas = atlas.clone();
                        cover_stamp = Some((next_cover_stamp, Arc::new(cover)));
                    }
                    // Replay the retained tile if an earlier notification arrived after a camera cut.
                    {
                        let mut state = shared.0.lock().unwrap();
                        if !state.stop
                            && state
                                .pending
                                .as_ref()
                                .is_none_or(|next| next.stamp == request.stamp)
                        {
                            state.cover = cover_stamp
                                .as_ref()
                                .map(|(_, tile)| (request.stamp, Arc::clone(tile)));
                        }
                    }
                    let snapshot =
                        match model.query(request.observer, request.utc, request.revision) {
                            Ok(snapshot) => snapshot,
                            Err(error) => {
                                fail(&shared, request.stamp, error);
                                continue;
                            }
                        };
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
                        sort(&mut publication, model.radius_m());
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

    pub(super) fn take_failure(&self) -> Option<(super::Stamp, GenerationError)> {
        self.mailbox.0.lock().unwrap().failure.take()
    }

    pub(super) fn take_cover(
        &self,
    ) -> Option<(
        super::Stamp,
        Arc<minimium_renderer::surface_cover::CoverGrid>,
    )> {
        self.mailbox.0.lock().unwrap().cover.take()
    }

    pub(super) fn clear(&self) {
        let mut state = self.mailbox.0.lock().unwrap();
        state.pending = None;
        state.completed = None;
        state.cover = None;
        state.failure = None;
    }
}

fn fail(shared: &Arc<(Mutex<Mailbox>, Condvar)>, stamp: super::Stamp, error: GenerationError) {
    let mut state = shared.0.lock().unwrap();
    if !state.stop
        && state
            .pending
            .as_ref()
            .is_none_or(|next| next.stamp == stamp)
    {
        state.failure = Some((stamp, error));
    }
}

impl Drop for CloudWorker {
    fn drop(&mut self) {
        self.mailbox.0.lock().unwrap().stop = true;
        self.mailbox.1.notify_one();
        self.thread.take();
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
