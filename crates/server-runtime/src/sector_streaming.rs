use crate::world_registry::WorldRegistry;
use minimium_core::ids::PlayerId;
use minimium_sectors::{SectorManager, SectorTicket};
use minimium_world::world::{GenerationRevision, SectorKey, WorldContentRevision, WorldInstanceId};
use std::{collections::HashSet, sync::Arc};

mod collision;
mod install;
mod readiness;
mod requests;
mod tickets;

pub(crate) type SectorPayload = minimium_procgen::SectorGeometry;

/// Request data selected from the current world registry snapshot.
struct PlayerSectorRequest {
    player: PlayerId,
    instance: WorldInstanceId,
    keys: Vec<SectorKey>,
    generation: GenerationRevision,
    content: WorldContentRevision,
}

struct TransferSectorRequest {
    player: PlayerId,
    instance: WorldInstanceId,
    position: [f32; 3],
    client_ready: bool,
    generation: GenerationRevision,
    content: WorldContentRevision,
}

struct PlayerReadiness {
    player: PlayerId,
    tickets: Vec<SectorTicket>,
    complete: bool,
}

struct TransferReadiness {
    player: PlayerId,
    tickets: Vec<SectorTicket>,
    complete: bool,
    client_ready: bool,
}

pub(crate) struct SectorReadiness {
    pub(crate) players: HashSet<PlayerId>,
    pub(crate) transfers: HashSet<PlayerId>,
}

#[cfg(test)]
pub(crate) fn update(
    worlds: &mut WorldRegistry,
    sectors: &mut SectorManager<SectorPayload>,
    active_tickets: &mut std::collections::HashMap<PlayerId, Vec<SectorTicket>>,
    transfer_tickets: &mut std::collections::HashMap<PlayerId, Vec<SectorTicket>>,
) -> SectorReadiness {
    update_with_geography(worlds, sectors, active_tickets, transfer_tickets, None)
}

pub(crate) fn update_with_geography(
    worlds: &mut WorldRegistry,
    sectors: &mut SectorManager<SectorPayload>,
    active_tickets: &mut std::collections::HashMap<PlayerId, Vec<SectorTicket>>,
    transfer_tickets: &mut std::collections::HashMap<PlayerId, Vec<SectorTicket>>,
    geography: Option<Arc<dyn minimium_procgen::PreparedGeography>>,
) -> SectorReadiness {
    let previous_active = std::mem::take(active_tickets);
    let previous_transfers = std::mem::take(transfer_tickets);
    let installed_before = collision::installed_tickets(sectors);

    let active_requests = requests::collect_active_requests(worlds);
    let active_readiness = tickets::reconcile_active_requests(
        sectors,
        active_tickets,
        previous_active,
        active_requests,
    );
    let transfer_requests = requests::collect_transfer_requests(worlds);
    let transfer_readiness = tickets::reconcile_transfer_requests(
        sectors,
        transfer_tickets,
        previous_transfers,
        transfer_requests,
        worlds,
    );

    install::install_pending(worlds, sectors, geography);
    let players = readiness::collect_ready_players(sectors, active_readiness);
    let transfers = readiness::collect_ready_transfers(sectors, transfer_readiness);

    let installed_after = collision::installed_tickets(sectors);
    let changed = collision::changed_instances(&installed_before, &installed_after);
    collision::update_changed_instances(worlds, sectors, changed);
    SectorReadiness { players, transfers }
}
