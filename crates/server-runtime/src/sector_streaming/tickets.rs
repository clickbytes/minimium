use super::{
    PlayerReadiness, PlayerSectorRequest, SectorPayload, TransferReadiness, TransferSectorRequest,
};
use crate::world_registry::WorldRegistry;
use minimium_core::ids::PlayerId;
use minimium_sectors::{RequestError, SectorManager, SectorTicket};
use minimium_world::world::{GenerationRevision, SectorKey, WorldContentRevision, WorldInstanceId};
use std::collections::{HashMap, HashSet};

pub(super) fn reconcile_active_requests(
    sectors: &mut SectorManager<SectorPayload>,
    active_tickets: &mut HashMap<PlayerId, Vec<SectorTicket>>,
    previous_active: HashMap<PlayerId, Vec<SectorTicket>>,
    requests: Vec<PlayerSectorRequest>,
    logger: &minimium_diagnostics::DomainLogger,
) -> Vec<PlayerReadiness> {
    let mut previous_active = previous_active;
    let mut readiness = Vec::with_capacity(requests.len());
    for request in requests {
        let previous = previous_active.remove(&request.player).unwrap_or_default();
        let (tickets, complete) = reconcile_tickets(
            sectors,
            previous,
            request.instance,
            request.keys,
            request.generation,
            request.content,
            logger,
        );
        active_tickets.insert(request.player, tickets.clone());
        readiness.push(PlayerReadiness {
            player: request.player,
            tickets,
            complete,
        });
    }
    release_cached_tickets(sectors, previous_active);
    readiness
}

pub(super) fn reconcile_transfer_requests(
    sectors: &mut SectorManager<SectorPayload>,
    transfer_tickets: &mut HashMap<PlayerId, Vec<SectorTicket>>,
    previous_transfers: HashMap<PlayerId, Vec<SectorTicket>>,
    requests: Vec<TransferSectorRequest>,
    worlds: &WorldRegistry,
    logger: &minimium_diagnostics::DomainLogger,
) -> Vec<TransferReadiness> {
    let mut previous_transfers = previous_transfers;
    let mut readiness = Vec::with_capacity(requests.len());
    for request in requests {
        let previous = previous_transfers
            .remove(&request.player)
            .unwrap_or_default();
        let manifest = worlds
            .get(request.instance)
            .map(|instance| instance.manifest);
        let Some(manifest) = manifest else {
            release_tickets(sectors, previous);
            readiness.push(TransferReadiness {
                player: request.player,
                tickets: Vec::new(),
                complete: false,
                client_ready: request.client_ready,
            });
            continue;
        };
        let (tickets, complete) = reconcile_tickets(
            sectors,
            previous,
            request.instance,
            minimium_procgen::sector_neighborhood_for(manifest, request.position),
            request.generation,
            request.content,
            logger,
        );
        transfer_tickets.insert(request.player, tickets.clone());
        readiness.push(TransferReadiness {
            player: request.player,
            tickets,
            complete,
            client_ready: request.client_ready,
        });
    }
    release_cached_tickets(sectors, previous_transfers);
    readiness
}

fn reconcile_tickets(
    sectors: &mut SectorManager<SectorPayload>,
    previous: Vec<SectorTicket>,
    instance: WorldInstanceId,
    keys: Vec<SectorKey>,
    generation: GenerationRevision,
    content: WorldContentRevision,
    logger: &minimium_diagnostics::DomainLogger,
) -> (Vec<SectorTicket>, bool) {
    let mut seen = HashSet::with_capacity(keys.len());
    let desired: Vec<_> = keys.into_iter().filter(|key| seen.insert(*key)).collect();
    let desired_keys: HashSet<_> = desired.iter().copied().collect();
    let mut retained = HashMap::with_capacity(previous.len());
    for ticket in previous {
        let matches = ticket.instance() == instance
            && ticket.generation() == generation
            && ticket.content_revision() == content
            && desired_keys.contains(&ticket.key())
            && sectors.state(ticket) != Some(minimium_sectors::SectorState::Failed);
        if matches {
            if let Some(displaced) = retained.insert(ticket.key(), ticket) {
                sectors.unpin(displaced);
            }
        } else {
            sectors.unpin(ticket);
            if sectors.state(ticket) == Some(minimium_sectors::SectorState::Failed) {
                let _ = sectors.evict(ticket);
            }
        }
    }

    let mut tickets = Vec::with_capacity(desired.len());
    let mut complete = true;
    for key in desired {
        if let Some(ticket) = retained.remove(&key) {
            tickets.push(ticket);
            continue;
        }
        let Some(ticket) = request_sector(sectors, instance, key, generation, content, logger)
        else {
            complete = false;
            continue;
        };
        if sectors.pin(ticket) {
            tickets.push(ticket);
        } else {
            complete = false;
        }
    }
    complete &= tickets.len() == desired_keys.len();
    (tickets, complete)
}

fn release_cached_tickets(
    sectors: &mut SectorManager<SectorPayload>,
    cached: HashMap<PlayerId, Vec<SectorTicket>>,
) {
    for tickets in cached.into_values() {
        release_tickets(sectors, tickets);
    }
}

fn release_tickets(sectors: &mut SectorManager<SectorPayload>, tickets: Vec<SectorTicket>) {
    for ticket in tickets {
        sectors.unpin(ticket);
    }
}

fn request_sector(
    sectors: &mut SectorManager<SectorPayload>,
    instance: WorldInstanceId,
    key: SectorKey,
    generation: GenerationRevision,
    content: WorldContentRevision,
    logger: &minimium_diagnostics::DomainLogger,
) -> Option<SectorTicket> {
    match sectors.request(instance, key, generation, content) {
        Ok(ticket) => Some(ticket),
        Err(RequestError::CacheFull) if sectors.evict_oldest() => {
            match sectors.request(instance, key, generation, content) {
                Ok(ticket) => Some(ticket),
                Err(error) => {
                    log_request_failure(logger, instance, key, &error);
                    None
                }
            }
        }
        Err(error) => {
            log_request_failure(logger, instance, key, &error);
            None
        }
    }
}

fn log_request_failure(
    logger: &minimium_diagnostics::DomainLogger,
    instance: WorldInstanceId,
    key: SectorKey,
    error: &RequestError,
) {
    crate::diagnostics::emit(
        logger,
        minimium_diagnostics::Level::Warn,
        "sectors.request_failed",
        &[
            ("world_instance", instance.to_string()),
            ("sector", format!("{key:?}")),
            ("reason", format!("{error:?}")),
        ],
    );
}
