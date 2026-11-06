//! Bounded sector materialization. SectorManager installation stays on the UI lane.
use minimium_procgen::{PreparedGeography, SectorGeometry};
use minimium_sectors::SectorTicket;
use minimium_world::world::{WorldManifest, WorldMap};
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};
struct Job {
    ticket: SectorTicket,
    map: WorldMap,
    manifest: WorldManifest,
    geography: Option<Arc<dyn PreparedGeography>>,
}
#[derive(Default)]
struct State {
    pending: VecDeque<Job>,
    completed: Vec<(SectorTicket, Arc<SectorGeometry>)>,
    busy: bool,
    stop: bool,
}
pub(super) struct CollisionWorker {
    state: Arc<(Mutex<State>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}
impl CollisionWorker {
    pub fn new() -> Self {
        let state = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let shared = state.clone();
        let thread = std::thread::Builder::new()
            .name("sector-materialization".into())
            .spawn(move || loop {
                let job = {
                    let mut state = shared.0.lock().unwrap();
                    while state.pending.is_empty() && !state.stop {
                        state = shared.1.wait(state).unwrap();
                    }
                    if state.stop {
                        break;
                    }
                    state.busy = true;
                    state.pending.pop_front().unwrap()
                };
                let geometry = Arc::new(minimium_procgen::materialize_world_sector_with_geography(
                    &job.map,
                    job.manifest,
                    job.ticket.key(),
                    job.geography,
                ));
                let mut state = shared.0.lock().unwrap();
                state.busy = false;
                if !state.stop {
                    state.completed.push((job.ticket, geometry));
                }
            })
            .expect("sector materialization worker");
        Self {
            state,
            thread: Some(thread),
        }
    }
    pub fn has_capacity(&self) -> bool {
        let s = self.state.0.lock().unwrap();
        s.pending.len() + s.completed.len() + usize::from(s.busy) < 32
    }
    pub fn submit(
        &self,
        ticket: SectorTicket,
        map: WorldMap,
        manifest: WorldManifest,
        geography: Option<Arc<dyn PreparedGeography>>,
    ) {
        self.state.0.lock().unwrap().pending.push_back(Job {
            ticket,
            map,
            manifest,
            geography,
        });
        self.state.1.notify_one();
    }
    pub fn poll(&self) -> Vec<(SectorTicket, Arc<SectorGeometry>)> {
        std::mem::take(&mut self.state.0.lock().unwrap().completed)
    }
}
impl Drop for CollisionWorker {
    fn drop(&mut self) {
        {
            let mut s = self.state.0.lock().unwrap();
            s.stop = true;
            s.pending.clear();
            self.state.1.notify_one();
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
