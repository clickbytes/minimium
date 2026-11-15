//! Bounded sector materialization. SectorManager installation stays on the UI lane.
use super::sector_scene::{SceneInterestOwner, SectorWorkerError};
use minimium_procgen::SectorGeometry;
use minimium_sectors::SectorTicket;
use minimium_world::world::{WorldManifest, WorldMap};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};

pub(super) struct SceneCollisionWorker {
    state: Arc<(Mutex<SceneCollisionState>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}
#[derive(Default)]
struct SceneCollisionState {
    cached: Vec<(
        Arc<SectorGeometry>,
        Arc<minimium_simulation::CollisionIndex>,
    )>,
    pending: HashMap<SceneInterestOwner, SceneCollisionJob>,
    completed: HashMap<SceneInterestOwner, SceneCollisionResult>,
    stop: bool,
}
struct SceneCollisionJob {
    owner: SceneInterestOwner,
    revision: u64,
    sectors: Vec<Arc<SectorGeometry>>,
}
pub(super) struct SceneCollisionResult {
    pub(super) owner: SceneInterestOwner,
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
            .spawn(move || loop {
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
                let previous = std::mem::take(&mut shared.0.lock().unwrap().cached);
                let cached: Vec<_> = job
                    .sectors
                    .iter()
                    .map(|geometry| {
                        let index = previous
                            .iter()
                            .find(|(old, _)| Arc::ptr_eq(old, geometry))
                            .map(|(_, index)| index.clone())
                            .unwrap_or_else(|| {
                                Arc::new(super::sector_scene::collision_from_sectors(
                                    std::iter::once(geometry.as_ref()),
                                ))
                            });
                        (geometry.clone(), index)
                    })
                    .collect();
                let index = Arc::new(minimium_simulation::CollisionIndex::from_partitions(
                    cached.iter().map(|(_, index)| index.clone()).collect(),
                ));
                let result = SceneCollisionResult {
                    owner: job.owner,
                    revision: job.revision,
                    index,
                };
                let mut state = shared.0.lock().unwrap();
                state.cached = cached;
                if !state.stop {
                    state.completed.insert(result.owner, result);
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
        owner: SceneInterestOwner,
        revision: u64,
        sectors: Vec<Arc<SectorGeometry>>,
    ) -> Result<(), SectorWorkerError> {
        if self.thread.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(SectorWorkerError::WorkerStopped(Vec::new()));
        }
        let mut state = self.state.0.lock().unwrap();
        if state.pending.len() + state.completed.len() >= 3
            && !state.pending.contains_key(&owner)
            && !state.completed.contains_key(&owner)
        {
            return Err(SectorWorkerError::QueueFull);
        }
        state.pending.insert(
            owner,
            SceneCollisionJob {
                owner,
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

    pub(super) fn cancel(&self, owner: SceneInterestOwner) {
        let mut state = self.state.0.lock().unwrap();
        state.pending.remove(&owner);
        state.completed.remove(&owner);
    }

    pub(super) fn promote(&self, source: SceneInterestOwner, target: SceneInterestOwner) {
        let mut state = self.state.0.lock().unwrap();
        if let Some(mut job) = state.pending.remove(&source) {
            job.owner = target;
            state.pending.insert(target, job);
        }
        if let Some(mut result) = state.completed.remove(&source) {
            result.owner = target;
            state.completed.insert(target, result);
        }
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
    geography: minimium_procgen::TerrainInputs,
}
#[derive(Default)]
struct State {
    pending: VecDeque<Job>,
    completed: Vec<(
        SectorTicket,
        Result<Arc<SectorGeometry>, minimium_procgen::ExcavationGeometryError>,
    )>,
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
                    let job = state.pending.pop_front().unwrap();
                    state.active = Some(job.ticket);
                    job
                };
                let geometry = minimium_procgen::materialize_world_sector_with_geography(
                    &job.map,
                    job.manifest,
                    job.ticket.key(),
                    job.geography,
                )
                .map(Arc::new);
                let mut state = shared.0.lock().unwrap();
                state.busy = false;
                state.active = None;
                if !state.stop {
                    state.completed.push((job.ticket, geometry));
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
        geography: minimium_procgen::TerrainInputs,
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
    pub fn poll(
        &self,
    ) -> Result<
        Vec<(
            SectorTicket,
            Result<Arc<SectorGeometry>, minimium_procgen::ExcavationGeometryError>,
        )>,
        SectorWorkerError,
    > {
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
