//! Compile-only consumer for the gameplay-owned world-change contract.
use minimium_gameplay::{
    WorldChangeCommitment, WorldChangeFailure, WorldChangePlan, WorldChangeState,
};
use minimium_storage::{FileStorageBackend, MemoryStorageBackend, StorageBackend};
use minimium_world::{items::Inventory, world::WorldInstanceId};
use std::{io, time::Instant};

fn report(result: Result<(), minimium_storage::excavation::PersistenceError>) {
    match result {
        Ok(()) => {}
        Err(error) => match error.kind {
            WorldChangeFailure::IdentityConflict | WorldChangeFailure::RevisionConflict => {}
            WorldChangeFailure::LimitExceeded => {}
            WorldChangeFailure::InvalidPlan => {}
            WorldChangeFailure::Busy => {}
            WorldChangeFailure::Unavailable | WorldChangeFailure::Failure => {
                let _diagnostic_source = error.source_error();
            }
        },
    }
}

fn consume_backend(
    backend: &dyn StorageBackend,
    plan: WorldChangePlan,
) -> io::Result<WorldChangeState> {
    let mut services = backend.open_services()?;
    report(services.excavation.validate(&plan));
    match services.excavation.submit(plan.clone()) {
        Ok(()) => {}
        Err(error) => report(Err(error)),
    }
    if let Some(result) = services.excavation.poll_acknowledged() {
        match result {
            Ok(ack) if ack.plan.same_plan(&plan) => match ack.commitment {
                WorldChangeCommitment::Durable | WorldChangeCommitment::Volatile => {}
            },
            Ok(_) => {}
            Err(error) => report(Err(error)),
        }
    }
    services
        .excavation
        .shutdown_until(Instant::now() + std::time::Duration::from_secs(1))?;
    Ok(services.excavation_restore)
}

/// The call graph type-checks against both shipped persistence adapters.
pub fn compile_consumer(
    file: &FileStorageBackend,
    memory: &MemoryStorageBackend,
) -> io::Result<()> {
    let id = WorldInstanceId::new(1).expect("nonzero world ID");
    let profile = minimium_core::identity::PlayerProfileId::from_bytes([7; 16]);
    let inventory = Inventory {
        revision: 1,
        ..Inventory::default()
    };
    let plan = WorldChangePlan::new(
        1,
        id,
        42,
        Vec::new(),
        Vec::new(),
        Some((profile, inventory)),
        None,
        Vec::new(),
        Vec::new(),
        None,
    )
    .expect("bounded inventory plan");
    let _file_state = consume_backend(file, plan.clone())?;
    let _memory_state = consume_backend(memory, plan)?;
    Ok(())
}

fn main() {}
