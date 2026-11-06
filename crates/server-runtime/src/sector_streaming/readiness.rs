use super::{PlayerReadiness, SectorPayload, TransferReadiness};
use minimium_core::ids::PlayerId;
use minimium_sectors::SectorManager;
use std::collections::HashSet;

pub(super) fn collect_ready_players(
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
                    .all(|ticket| sectors.collision_ready(*ticket)))
            .then_some(request.player)
        })
        .collect()
}

pub(super) fn collect_ready_transfers(
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
                    .all(|ticket| sectors.collision_ready(*ticket)))
            .then_some(request.player)
        })
        .collect()
}
