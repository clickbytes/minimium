//! Bounded sector materialization. SectorManager installation stays on the UI lane.
use super::sector_scene::SectorWorkerError;
use minimium_procgen::{PreparedGeography, SectorGeometry};
use minimium_sectors::SectorTicket;
use minimium_world::world::{WorldManifest, WorldMap};
use std::{
    collections::BTreeMap,
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

pub(super) struct SceneCollisionWorker {
    state: Arc<(Mutex<SceneCollisionState>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}
#[derive(Default)]
struct SceneCollisionState {
    pending: BTreeMap<minimium_world::world::WorldInstanceId, SceneCollisionJob>,
    completed: BTreeMap<minimium_world::world::WorldInstanceId, SceneCollisionResult>,
    stop: bool,
}
struct SceneCollisionJob {
    instance: minimium_world::world::WorldInstanceId,
    revision: u64,
    sectors: Vec<Arc<SectorGeometry>>,
}
pub(super) struct SceneCollisionResult {
    pub(super) instance: minimium_world::world::WorldInstanceId,
    pub(super) revision: u64,
    pub(super) index: Arc<minimium_simulation::CollisionIndex>,
}
impl SceneCollisionWorker {
    pub(super) fn shutdown_until(&mut self, deadline: std::time::Instant) -> bool {
        {
            let mut state = self.state.0.lock().unwrap();
            state.stop = true;
            state.pending.clear();
            self.state.1.notify_all();
        }
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

    pub(super) fn new() -> Result<Self, SectorWorkerError> {
        let state = Arc::new((Mutex::new(SceneCollisionState::default()), Condvar::new()));
        let shared = state.clone();
        let thread = std::thread::Builder::new()
            .name("client-collision-index".into())
            .spawn(move || {
                loop {
                    let job = {
                        let mut state = shared.0.lock().unwrap();
                        while state.pending.is_empty() && !state.stop {
                            state = shared.1.wait(state).unwrap();
                        }
                        if state.stop {
                            return;
                        }
                        let key = *state.pending.keys().next().expect("pending collision job");
                        state.pending.remove(&key).expect("selected collision job")
                    };
                    let index = Arc::new(super::sector_scene::collision_from_sectors(
                        job.sectors.iter().map(AsRef::as_ref),
                    ));
                    let result = SceneCollisionResult {
                        instance: job.instance,
                        revision: job.revision,
                        index,
                    };
                    let mut state = shared.0.lock().unwrap();
                    if !state.stop {
                        state.completed.insert(result.instance, result);
                    }
                }
            })
            .map_err(SectorWorkerError::ThreadSpawn)?;
        Ok(Self {
            state,
            thread: Some(thread),
        })
    }
    pub(super) fn submit(
        &self,
        instance: minimium_world::world::WorldInstanceId,
        revision: u64,
        sectors: Vec<Arc<SectorGeometry>>,
    ) -> Result<(), SectorWorkerError> {
        if self.thread.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(SectorWorkerError::WorkerStopped(Vec::new()));
        }
        self.state.0.lock().unwrap().pending.insert(
            instance,
            SceneCollisionJob {
                instance,
                revision,
                sectors,
            },
        );
        self.state.1.notify_one();
        Ok(())
    }
    pub(super) fn poll(&self) -> Result<Vec<SceneCollisionResult>, SectorWorkerError> {
        if self.thread.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(SectorWorkerError::WorkerStopped(Vec::new()));
        }
        Ok(std::mem::take(&mut self.state.0.lock().unwrap().completed)
            .into_values()
            .collect())
    }
}
impl Drop for SceneCollisionWorker {
    fn drop(&mut self) {
        {
            let mut state = self.state.0.lock().unwrap();
            state.stop = true;
            state.pending.clear();
            self.state.1.notify_all();
        }
        self.thread.take();
    }
}
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
    active: Option<SectorTicket>,
    stop: bool,
}
pub(super) struct CollisionWorker {
    state: Arc<(Mutex<State>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}
impl CollisionWorker {
    pub(super) fn shutdown_until(&mut self, deadline: std::time::Instant) -> bool {
        {
            let mut state = self.state.0.lock().unwrap();
            state.stop = true;
            state.pending.clear();
            self.state.1.notify_one();
        }
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
            .name("sector-materialization".into())
            .spawn(move || {
                loop {
                    let job = {
                        let mut state = shared.0.lock().unwrap();
                        while state.pending.is_empty() && !state.stop {
                            state = shared.1.wait(state).unwrap();
                        }
                        if state.stop {
                            break;
                        }
                        state.busy = true;
                        let job = state.pending.pop_front().unwrap();
                        state.active = Some(job.ticket);
                        job
                    };
                    let geometry =
                        Arc::new(minimium_procgen::materialize_world_sector_with_geography(
                            &job.map,
                            job.manifest,
                            job.ticket.key(),
                            job.geography,
                        ));
                    let mut state = shared.0.lock().unwrap();
                    state.busy = false;
                    state.active = None;
                    if !state.stop {
                        state.completed.push((job.ticket, geometry));
                    }
                }
            })
            .map_err(SectorWorkerError::ThreadSpawn)?;
        Ok(Self {
            state,
            thread: Some(thread),
        })
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
    ) -> Result<(), SectorWorkerError> {
        let mut state = self.state.0.lock().unwrap();
        if state.stop || self.thread.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(SectorWorkerError::WorkerStopped(vec![ticket]));
        }
        if state.pending.len() + state.completed.len() + usize::from(state.busy) >= 32 {
            return Err(SectorWorkerError::QueueFull);
        }
        state.pending.push_back(Job {
            ticket,
            map,
            manifest,
            geography,
        });
        drop(state);
        self.state.1.notify_one();
        Ok(())
    }
    pub fn poll(&self) -> Result<Vec<(SectorTicket, Arc<SectorGeometry>)>, SectorWorkerError> {
        if self.thread.as_ref().is_none_or(JoinHandle::is_finished) {
            let mut state = self.state.0.lock().unwrap();
            let mut failed: Vec<_> = state.pending.drain(..).map(|job| job.ticket).collect();
            failed.extend(state.active.take());
            failed.extend(
                std::mem::take(&mut state.completed)
                    .into_iter()
                    .map(|(ticket, _)| ticket),
            );
            return Err(SectorWorkerError::WorkerStopped(failed));
        }
        Ok(std::mem::take(&mut self.state.0.lock().unwrap().completed))
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
        self.thread.take();
    }
}
