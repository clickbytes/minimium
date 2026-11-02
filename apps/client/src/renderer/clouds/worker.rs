use super::{cache::MeshCache, Publication, Request};
use crate::environment::weather::{SeasonalProfile, WeatherModel};
use std::sync::{Arc, Condvar, Mutex};

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
                let mut model: Option<WeatherModel> = None;
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
                        model =
                            WeatherModel::new(request.stamp.manifest, SeasonalProfile::AnnualMean);
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
