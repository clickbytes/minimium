use super::SectorPayload;
use crate::world_registry::WorldRegistry;
use minimium_diagnostics::{DomainLogger, Level};
use minimium_sectors::{SectorManager, SectorTicket};
use minimium_world::world::WorldInstanceId;
use std::{
    collections::{BTreeMap, HashSet},
    io,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

struct CollisionJob {
    instance: WorldInstanceId,
    revision: u64,
    sectors: Vec<(SectorTicket, Arc<SectorPayload>)>,
}

struct CollisionResult {
    tickets: HashSet<SectorTicket>,
    instance: WorldInstanceId,
    revision: u64,
    index: Arc<minimium_simulation::CollisionIndex>,
}

#[derive(Default)]
struct State {
    pending: BTreeMap<WorldInstanceId, CollisionJob>,
    completed: BTreeMap<WorldInstanceId, CollisionResult>,
    active: Option<WorldInstanceId>,
    stop: bool,
}

pub(crate) struct CollisionIndexWorker {
    shared: Arc<(Mutex<State>, Condvar)>,
    thread: Option<JoinHandle<()>>,
}

impl CollisionIndexWorker {
    pub(crate) fn new() -> io::Result<Self> {
        let shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let thread_shared = shared.clone();
        let thread = thread::Builder::new()
            .name("server-collision-index".into())
            .spawn(move || collision_loop(thread_shared))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }

    pub(crate) fn shutdown_until(&mut self, deadline: Instant) -> Vec<String> {
        self.request_stop();
        while self
            .thread
            .as_ref()
            .is_some_and(|thread| !thread.is_finished())
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(5));
        }
        match self.thread.take() {
            Some(thread) if thread.is_finished() => {
                if thread.join().is_err() {
                    vec!["collision-index-panicked".to_owned()]
                } else {
                    Vec::new()
                }
            }
            Some(_) => vec!["collision-index".to_owned()],
            None => Vec::new(),
        }
    }

    fn request_stop(&mut self) {
        let mut state = self.shared.0.lock().unwrap();
        state.stop = true;
        state.pending.clear();
        self.shared.1.notify_all();
    }

    fn submit(&self, job: CollisionJob) -> Result<(), ()> {
        if self.thread.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(());
        }
        self.shared
            .0
            .lock()
            .unwrap()
            .pending
            .insert(job.instance, job);
        self.shared.1.notify_one();
        Ok(())
    }

    fn poll(&self) -> Result<Vec<CollisionResult>, ()> {
        if self.thread.as_ref().is_none_or(JoinHandle::is_finished) {
            return Err(());
        }
        Ok(std::mem::take(&mut self.shared.0.lock().unwrap().completed)
            .into_values()
            .collect())
    }
}

impl Drop for CollisionIndexWorker {
    fn drop(&mut self) {
        self.request_stop();
        self.thread.take();
    }
}

pub(super) fn installed_tickets(sectors: &SectorManager<SectorPayload>) -> HashSet<SectorTicket> {
    sectors.installed().map(|(ticket, _, _)| ticket).collect()
}

pub(super) fn changed_instances(
    installed_before: &HashSet<SectorTicket>,
    installed_after: &HashSet<SectorTicket>,
) -> HashSet<WorldInstanceId> {
    installed_before
        .symmetric_difference(installed_after)
        .map(|ticket| ticket.instance())
        .collect()
}

pub(super) fn update_changed_instances(
    worlds: &mut WorldRegistry,
    sectors: &SectorManager<SectorPayload>,
    changed: HashSet<WorldInstanceId>,
    worker: &CollisionIndexWorker,
    logger: &DomainLogger,
) {
    for instance in changed {
        let Some(owner) = worlds.get_mut(instance) else {
            continue;
        };
        let Some(revision) = owner.collision_source_revision.checked_add(1) else {
            emit_collision_failure(logger, "sectors.collision_revision_exhausted", instance);
            continue;
        };
        owner.collision_source_revision = revision;
        let snapshot = sectors
            .installed()
            .filter(|(ticket, _, _)| ticket.instance() == instance)
            .filter_map(|(ticket, _, _)| {
                sectors.payload_arc(ticket).map(|payload| (ticket, payload))
            })
            .collect();
        if worker
            .submit(CollisionJob {
                instance,
                revision,
                sectors: snapshot,
            })
            .is_err()
        {
            emit_collision_failure(logger, "sectors.collision_worker_stopped", instance);
        }
    }
    let results = match worker.poll() {
        Ok(results) => results,
        Err(()) => {
            for (instance, _) in worlds.iter() {
                if let Some(world) = worlds.get(instance) {
                    if world.collision_installed_revision != world.collision_source_revision {
                        emit_collision_failure(
                            logger,
                            "sectors.collision_worker_stopped",
                            instance,
                        );
                    }
                }
            }
            return;
        }
    };
    for result in results {
        let Some(owner) = worlds.get_mut(result.instance) else {
            continue;
        };
        if owner.collision_source_revision != result.revision {
            continue;
        }
        owner.world.install_streamed_collision_index(result.index);
        owner.collision_tickets = result.tickets;
        owner.collision_installed_revision = result.revision;
    }
}

pub(super) fn tickets_ready(worlds: &WorldRegistry, tickets: &[SectorTicket]) -> bool {
    tickets
        .first()
        .and_then(|t| worlds.get(t.instance()))
        .is_some_and(|w| {
            tickets
                .iter()
                .all(|ticket| w.collision_tickets.contains(ticket))
        })
}

fn collision_loop(shared: Arc<(Mutex<State>, Condvar)>) {
    loop {
        let job = {
            let mut state = shared.0.lock().unwrap();
            while state.pending.is_empty() && !state.stop {
                state = shared.1.wait(state).unwrap();
            }
            if state.stop {
                return;
            }
            let instance = *state
                .pending
                .keys()
                .next()
                .expect("pending collision build");
            let job = state
                .pending
                .remove(&instance)
                .expect("selected collision build");
            state.active = Some(instance);
            job
        };
        let tickets: HashSet<_> = job.sectors.iter().map(|(ticket, _)| *ticket).collect();
        // Each sector owns its prepared index; rebuilding the root copies Arcs.
        let parts = job
            .sectors
            .iter()
            .map(|(_, payload)| payload.collision.clone())
            .collect();
        let result = CollisionResult {
            tickets,
            instance: job.instance,
            revision: job.revision,
            index: Arc::new(minimium_simulation::CollisionIndex::from_partitions(parts)),
        };
        let mut state = shared.0.lock().unwrap();
        state.active = None;
        if !state.stop {
            state.completed.insert(result.instance, result);
        }
    }
}

fn emit_collision_failure(logger: &DomainLogger, code: &'static str, instance: WorldInstanceId) {
    crate::diagnostics::emit(
        logger,
        Level::Error,
        code,
        &[("instance", instance.get().to_string())],
    );
}
