use super::*;
use minimium_core::world::{GenerationRevision, PlanetFace, SectorRevision, WorldContentRevision};

fn revisions(generation: u32, content: u64) -> (GenerationRevision, WorldContentRevision) {
    (
        GenerationRevision::new(generation).unwrap(),
        WorldContentRevision::new(content).unwrap(),
    )
}

#[test]
fn ticket_replacement_rejects_late_generation_and_requires_collision_ready() {
    let instance = WorldInstanceId::new(1).unwrap();
    let key = SectorKey::planet(PlanetFace::PositiveY, 4, 9).unwrap();
    let mut sectors = SectorManager::new(2, 1).unwrap();
    let (generation, content) = revisions(1, 1);
    let old = sectors.request(instance, key, generation, content).unwrap();
    assert_eq!(sectors.request(instance, key, generation, content), Ok(old));
    assert_eq!(sectors.pending_count(), 1);
    assert_eq!(sectors.begin_next(), Some(old));

    let (new_generation, _) = revisions(2, 1);
    let new = sectors
        .request(instance, key, new_generation, content)
        .unwrap();
    assert_ne!(old, new);
    assert_eq!(
        sectors.install_data(old, SectorRevision::new(1).unwrap(), Arc::new(3)),
        InstallResult::Stale
    );
    assert_eq!(sectors.begin_next(), Some(new));
    assert_eq!(
        sectors.install_data(new, SectorRevision::new(1).unwrap(), Arc::new(7)),
        InstallResult::Installed
    );
    assert_eq!(sectors.state(new), Some(SectorState::DataReady));
    assert_eq!(sectors.installed().count(), 0);
    assert_eq!(sectors.install_collision(new), InstallResult::Installed);
    assert_eq!(sectors.installed().next().unwrap().2, &7);
}

#[test]
fn cache_is_bounded_isolated_by_instance_and_evicts_only_clean_unpinned_entries() {
    let first = WorldInstanceId::new(1).unwrap();
    let second = WorldInstanceId::new(2).unwrap();
    let key = SectorKey::dungeon(0, 0, 0);
    let (generation, content) = revisions(1, 1);
    let mut sectors = SectorManager::new(2, 2).unwrap();
    let first_ticket = sectors.request(first, key, generation, content).unwrap();
    let second_ticket = sectors.request(second, key, generation, content).unwrap();
    assert_ne!(first_ticket, second_ticket);
    assert_eq!(sectors.begin_next(), Some(first_ticket));
    assert_eq!(
        sectors.install_data(first_ticket, SectorRevision::new(1).unwrap(), Arc::new(11)),
        InstallResult::Installed
    );
    sectors.install_collision(first_ticket);
    assert_eq!(sectors.begin_next(), Some(second_ticket));
    assert_eq!(
        sectors.install_data(second_ticket, SectorRevision::new(1).unwrap(), Arc::new(22)),
        InstallResult::Installed
    );
    sectors.install_collision(second_ticket);

    assert_eq!(sectors.payload(first_ticket), Some(&11));
    assert_eq!(sectors.payload(second_ticket), Some(&22));
    assert!(sectors.pin(first_ticket));
    assert!(!sectors.evict(first_ticket));
    assert!(sectors.unpin(first_ticket));
    assert!(sectors.mark_dirty(first_ticket));
    assert!(!sectors.evict(first_ticket));
    assert!(sectors.mark_saved(first_ticket, SectorRevision::new(1).unwrap()));
    assert!(sectors.evict(first_ticket));
    assert!(sectors.evict(second_ticket));

    let queued = sectors.request(first, key, generation, content).unwrap();
    let queued_second = sectors.request(second, key, generation, content).unwrap();
    assert_eq!(sectors.pending_count(), 2);
    assert_eq!(
        sectors.request(WorldInstanceId::new(3).unwrap(), key, generation, content),
        Err(RequestError::QueueFull)
    );
    assert_eq!(sectors.state(queued), Some(SectorState::Queued));
    assert_eq!(sectors.state(queued_second), Some(SectorState::Queued));
}

#[test]
fn invalid_capacities_and_ticket_states_are_rejected() {
    assert!(matches!(
        SectorManager::<u8>::new(0, 1),
        Err(RequestError::InvalidCapacity)
    ));
    let instance = WorldInstanceId::new(3).unwrap();
    let key = SectorKey::dungeon(0, 0, 0);
    let (generation, content) = revisions(1, 1);
    let mut sectors = SectorManager::<u8>::new(1, 1).unwrap();
    let ticket = sectors.request(instance, key, generation, content).unwrap();
    assert_eq!(sectors.install_collision(ticket), InstallResult::WrongState);
    assert!(sectors.begin_next().is_some());
    assert_eq!(sectors.install_collision(ticket), InstallResult::WrongState);
}
