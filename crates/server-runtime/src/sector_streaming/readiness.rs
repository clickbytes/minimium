use super::{PlayerReadiness, SectorPayload, TransferReadiness};
use crate::world_registry::WorldRegistry;
use minimium_core::ids::PlayerId;
use minimium_sectors::SectorManager;
use std::collections::HashSet;

pub(super) fn collect_ready_players(
    worlds: &WorldRegistry,
    sectors: &SectorManager<SectorPayload>,
    readiness: Vec<PlayerReadiness>,
) -> HashSet<PlayerId> {
    readiness
        .into_iter()
        .filter_map(|request| {
            (request.complete
                && request
                    .tickets
                    .iter()
                    .all(|ticket| sectors.collision_ready(*ticket))
                && request.tickets.first().is_some_and(|ticket| {
                    super::collision::instance_ready(worlds, ticket.instance())
                }))
            .then_some(request.player)
        })
        .collect()
}

pub(super) fn collect_ready_transfers(
    worlds: &WorldRegistry,
    sectors: &SectorManager<SectorPayload>,
    readiness: Vec<TransferReadiness>,
) -> HashSet<PlayerId> {
    readiness
        .into_iter()
        .filter_map(|request| {
            (request.client_ready
                && request.complete
                && !request.tickets.is_empty()
                && request
                    .tickets
                    .iter()
                    .all(|ticket| sectors.collision_ready(*ticket))
                && request.tickets.first().is_some_and(|ticket| {
                    super::collision::instance_ready(worlds, ticket.instance())
                }))
            .then_some(request.player)
        })
        .collect()
}
