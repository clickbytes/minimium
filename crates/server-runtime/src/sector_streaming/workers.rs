use super::SectorPayload;
use crate::world_registry::WorldRegistry;
use minimium_diagnostics::{DomainLogger, Level};
use minimium_sectors::{InstallResult, SectorManager, SectorTicket};
use minimium_world::world::{SectorRevision, WorldManifest, WorldMap};
use std::{
    collections::{HashMap, VecDeque},
    io,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const QUEUE_CAPACITY: usize = 64;
const MAX_INSTALLS_PER_TICK: usize = 8;
const MAX_SUBMITS_PER_TICK: usize = 64;

struct Job {
    ticket: SectorTicket,
    map: Arc<WorldMap>,
    manifest: WorldManifest,
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
}

struct Finished {
    ticket: SectorTicket,
    result: Result<Arc<SectorPayload>, minimium_procgen::GenerationError>,
}

#[derive(Default)]
struct State {
    pending: VecDeque<Job>,
    finished: VecDeque<Finished>,
    active: HashMap<usize, SectorTicket>,
    stop: bool,
}

pub(crate) struct SectorGenerationPool {
    shared: Arc<(Mutex<State>, Condvar)>,
    workers: Vec<(usize, JoinHandle<()>)>,
    collision: super::collision::CollisionIndexWorker,
}

impl SectorGenerationPool {
    pub(crate) fn new(worker_count: usize) -> io::Result<Self> {
        let collision = super::collision::CollisionIndexWorker::new()?;
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let mut workers = Vec::with_capacity(worker_count);
        for id in 0..worker_count {
            let thread_shared = shared.clone();
            let handle = thread::Builder::new()
                .name(format!("server-sector-{id}"))
                .spawn(move || worker_loop(id, thread_shared));
            match handle {
                Ok(handle) => workers.push((id, handle)),
                Err(error) => {
                    {
                        let mut state = shared.0.lock().unwrap();
                        state.stop = true;
                        shared.1.notify_all();
                    }
                    for (_, worker) in workers {
                        let _ = worker.join();
                    }
                    return Err(error);
                }
            }
        }
        Ok(Self {
            shared,
            workers,
            collision,
        })
    }

    pub(crate) fn shutdown_until(&mut self, deadline: Instant) -> Vec<String> {
        {
            let mut state = self.shared.0.lock().unwrap();
            state.stop = true;
            state.pending.clear();
            state.finished.clear();
            self.shared.1.notify_all();
        }
        let mut unfinished = Vec::new();
        while !self.workers.is_empty() && Instant::now() < deadline {
            let mut index = 0;
            while index < self.workers.len() {
                if self.workers[index].1.is_finished() {
                    let (id, worker) = self.workers.swap_remove(index);
                    if worker.join().is_err() {
                        unfinished.push(format!("server-sector-{id}-panicked"));
                    }
                } else {
                    index += 1;
                }
            }
            if !self.workers.is_empty() {
                thread::sleep(Duration::from_millis(5));
            }
        }
        unfinished.extend(
            self.workers
                .drain(..)
                .map(|(id, _)| format!("server-sector-{id}")),
        );
        unfinished.extend(self.collision.shutdown_until(deadline));
        unfinished
    }

    pub(super) fn update_collision(
        &self,
        worlds: &mut WorldRegistry,
        sectors: &SectorManager<SectorPayload>,
        changed: std::collections::HashSet<minimium_world::world::WorldInstanceId>,
        logger: &DomainLogger,
    ) {
        super::collision::update_changed_instances(
            worlds,
            sectors,
            changed,
            &self.collision,
            logger,
        );
    }

    pub(crate) fn update(
        &mut self,
        worlds: &WorldRegistry,
        sectors: &mut SectorManager<SectorPayload>,
        geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
        logger: &DomainLogger,
    ) {
        self.collect_worker_exits(sectors, logger);
        self.install_finished(sectors, logger);
        if self.workers.is_empty() {
            let abandoned: Vec<_> = self
                .shared
                .0
                .lock()
                .unwrap()
                .pending
                .drain(..)
                .map(|job| job.ticket)
                .collect();
            for ticket in abandoned {
                let _ = sectors.fail(ticket);
                emit_worker_error(logger, "sectors.no_generation_workers", ticket);
            }
            for _ in 0..MAX_SUBMITS_PER_TICK {
                let Some(ticket) = sectors.begin_next() else {
                    break;
                };
                let _ = sectors.fail(ticket);
                emit_worker_error(logger, "sectors.no_generation_workers", ticket);
            }
            return;
        }
        self.submit_pending(worlds, sectors, geography, logger);
    }

    fn collect_worker_exits(
        &mut self,
        sectors: &mut SectorManager<SectorPayload>,
        logger: &DomainLogger,
    ) {
        let mut index = 0;
        while index < self.workers.len() {
            if !self.workers[index].1.is_finished() {
                index += 1;
                continue;
            }
            let (id, worker) = self.workers.swap_remove(index);
            let _ = worker.join();
            let ticket = self.shared.0.lock().unwrap().active.remove(&id);
            if let Some(ticket) = ticket {
                let _ = sectors.fail(ticket);
                emit_worker_error(logger, "sectors.worker_stopped", ticket);
            }
        }
    }

    fn install_finished(&self, sectors: &mut SectorManager<SectorPayload>, logger: &DomainLogger) {
        let finished: Vec<_> = {
            let mut state = self.shared.0.lock().unwrap();
            let count = MAX_INSTALLS_PER_TICK.min(state.finished.len());
            state.finished.drain(..count).collect()
        };
        for outcome in finished {
            if !sectors.is_pinned(outcome.ticket) {
                let _ = sectors.cancel(outcome.ticket);
                continue;
            }
            match outcome.result {
                Ok(payload) => {
                    let revision =
                        SectorRevision::new(1).expect("initial sector revision is nonzero");
                    match sectors.complete(outcome.ticket, revision, payload) {
                        InstallResult::Installed => {
                            let _ = sectors.install_collision(outcome.ticket);
                        }
                        InstallResult::Stale | InstallResult::WrongState => {}
                    }
                }
                Err(error) => {
                    let _ = sectors.fail(outcome.ticket);
                    crate::diagnostics::emit(
                        logger,
                        Level::Error,
                        error.code(),
                        &[
                            ("ticket", format!("{:?}", outcome.ticket)),
                            ("reason", error.to_string()),
                        ],
                    );
                }
            }
        }
    }

    fn submit_pending(
        &self,
        worlds: &WorldRegistry,
        sectors: &mut SectorManager<SectorPayload>,
        geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
        logger: &DomainLogger,
    ) {
        for _ in 0..MAX_SUBMITS_PER_TICK {
            let outstanding = {
                let state = self.shared.0.lock().unwrap();
                state.pending.len() + state.finished.len() + state.active.len()
            };
            if outstanding >= QUEUE_CAPACITY {
                break;
            }
            let Some(ticket) = sectors.begin_next() else {
                break;
            };
            let Some(instance) = worlds.get(ticket.instance()) else {
                let _ = sectors.fail(ticket);
                emit_worker_error(logger, "sectors.world_missing_for_ticket", ticket);
                continue;
            };
            let job = Job {
                ticket,
                map: instance.map.clone(),
                manifest: instance.manifest,
                geography: geography.clone(),
            };
            let mut state = self.shared.0.lock().unwrap();
            if state.stop
                || state.pending.len() + state.finished.len() + state.active.len() >= QUEUE_CAPACITY
            {
                let _ = sectors.fail(ticket);
                emit_worker_error(logger, "sectors.worker_queue_full", ticket);
                break;
            }
            state.pending.push_back(job);
            drop(state);
            self.shared.1.notify_one();
        }
    }
}

impl Drop for SectorGenerationPool {
    fn drop(&mut self) {
        {
            let mut state = self.shared.0.lock().unwrap();
            state.stop = true;
            state.pending.clear();
            self.shared.1.notify_all();
        }
        self.workers.clear();
    }
}

fn worker_loop(id: usize, shared: Arc<(Mutex<State>, Condvar)>) {
    loop {
        let job = {
            let mut state = shared.0.lock().unwrap();
            while state.pending.is_empty() && !state.stop {
                state = shared.1.wait(state).unwrap();
            }
            if state.stop {
                return;
            }
            let job = state
                .pending
                .pop_front()
                .expect("pending sector job exists");
            state.active.insert(id, job.ticket);
            job
        };
        let validation = if job.manifest.source() == minimium_world::world::WorldSource::Authored {
            if job.manifest.generator_digest()
                == minimium_world::world::GeneratorDigest::authored_map_fingerprint(
                    job.map.fingerprint(),
                )
            {
                Ok(())
            } else {
                Err(minimium_procgen::GenerationError::invalid_manifest(
                    "authored-map digest does not match its map",
                ))
            }
        } else {
            minimium_procgen::validate_world_manifest(job.manifest)
        };
        let result = validation.map(|()| {
            Arc::new(minimium_procgen::materialize_world_sector_with_geography(
                &job.map,
                job.manifest,
                job.ticket.key(),
                job.geography,
            ))
        });
        let mut state = shared.0.lock().unwrap();
        state.active.remove(&id);
        if !state.stop {
            state.finished.push_back(Finished {
                ticket: job.ticket,
                result,
            });
        }
    }
}

fn emit_worker_error(logger: &DomainLogger, code: &'static str, ticket: SectorTicket) {
    crate::diagnostics::emit(
        logger,
        Level::Error,
        code,
        &[
            ("instance", ticket.instance().get().to_string()),
            ("sector", format!("{:?}", ticket.key())),
            ("ticket", format!("{ticket:?}")),
        ],
    );
}
