use crate::world_registry::WorldRegistry;
use minimium_core::{
    ids::PlayerId,
    world::{
        Aabb, GenerationRevision, MapBox, MapTriangle, SectorKey, SectorRevision,
        WorldContentRevision, WorldInstanceId,
    },
};
use minimium_sectors::{InstallResult, RequestError, SectorManager, SectorTicket};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

const MAX_INSTALLS_PER_TICK: usize = 32;

pub(crate) type SectorPayload = minimium_procgen::SectorGeometry;

/// Selects topology-specific neighborhoods and installs bounded, revisioned
/// geometry slices on the server's single mutation lane.
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

pub(crate) struct SectorReadiness {
    pub(crate) players: HashSet<PlayerId>,
    pub(crate) transfers: HashSet<PlayerId>,
}

pub(crate) fn update(
    worlds: &mut WorldRegistry,
    sectors: &mut SectorManager<SectorPayload>,
    active_tickets: &mut std::collections::HashMap<PlayerId, Vec<SectorTicket>>,
    transfer_tickets: &mut std::collections::HashMap<PlayerId, Vec<SectorTicket>>,
) -> SectorReadiness {
    let mut previous_active = std::mem::take(active_tickets);
    let mut previous_transfers = std::mem::take(transfer_tickets);

    let installed_before: HashSet<_> = sectors.installed().map(|(ticket, _, _)| ticket).collect();
    let mut requests = Vec::new();
    for (instance_id, instance) in worlds.iter() {
        let manifest = instance.manifest;
        let mut players = instance.world.snapshot();
        players.sort_unstable_by_key(|player| player.id);
        requests.extend(players.into_iter().map(|player| PlayerSectorRequest {
            player: player.id,
            instance: instance_id,
            keys: minimium_procgen::sector_neighborhood_for(manifest, player.position),
            generation: manifest.generation().revision(),
            content: manifest.content_revision(),
        }));
    }
    requests.sort_unstable_by_key(|request| request.player);

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
        );
        active_tickets.insert(request.player, tickets.clone());
        readiness.push((request.player, tickets, complete));
    }
    release_cached_tickets(sectors, previous_active);

    let mut transfer_requests: Vec<_> = worlds
        .pending_transfer_sector_requests()
        .into_iter()
        .filter_map(|(player, instance, position, client_ready)| {
            let manifest = worlds.get(instance)?.manifest;
            Some(TransferSectorRequest {
                player,
                instance,
                position,
                client_ready,
                generation: manifest.generation().revision(),
                content: manifest.content_revision(),
            })
        })
        .collect();
    transfer_requests.sort_unstable_by_key(|request| request.player);
    let mut transfer_readiness = Vec::with_capacity(transfer_requests.len());
    for request in transfer_requests {
        let previous = previous_transfers
            .remove(&request.player)
            .unwrap_or_default();
        let manifest = worlds
            .get(request.instance)
            .map(|instance| instance.manifest);
        let Some(manifest) = manifest else {
            release_tickets(sectors, previous);
            transfer_readiness.push((request.player, Vec::new(), false, request.client_ready));
            continue;
        };
        let (tickets, complete) = reconcile_tickets(
            sectors,
            previous,
            request.instance,
            minimium_procgen::sector_neighborhood_for(manifest, request.position),
            request.generation,
            request.content,
        );
        transfer_tickets.insert(request.player, tickets.clone());
        transfer_readiness.push((request.player, tickets, complete, request.client_ready));
    }
    release_cached_tickets(sectors, previous_transfers);

    for _ in 0..MAX_INSTALLS_PER_TICK {
        let Some(ticket) = sectors.begin_next() else {
            break;
        };
        let Some(instance) = worlds.get(ticket.instance()) else {
            continue;
        };
        let payload = Arc::new(minimium_procgen::materialize_world_sector(
            &instance.map,
            instance.manifest,
            ticket.key(),
        ));
        let revision = SectorRevision::new(1).expect("initial sector revision is nonzero");
        if sectors.install_data(ticket, revision, payload) == InstallResult::Installed {
            let _ = sectors.install_collision(ticket);
        }
    }

    let ready_players = readiness
        .into_iter()
        .filter_map(|(player, tickets, complete)| {
            (complete
                && tickets
                    .iter()
                    .all(|ticket| sectors.collision_ready(*ticket)))
            .then_some(player)
        })
        .collect();
    let ready_transfers = transfer_readiness
        .into_iter()
        .filter_map(|(player, tickets, complete, client_ready)| {
            (client_ready
                && complete
                && !tickets.is_empty()
                && tickets
                    .iter()
                    .all(|ticket| sectors.collision_ready(*ticket)))
            .then_some(player)
        })
        .collect();

    let installed_after: HashSet<_> = sectors.installed().map(|(ticket, _, _)| ticket).collect();
    let changed_instances: HashSet<_> = installed_before
        .symmetric_difference(&installed_after)
        .map(|ticket| ticket.instance())
        .collect();
    for instance in changed_instances {
        install_instance_collision(worlds, sectors, instance);
    }
    SectorReadiness {
        players: ready_players,
        transfers: ready_transfers,
    }
}

fn reconcile_tickets(
    sectors: &mut SectorManager<SectorPayload>,
    previous: Vec<SectorTicket>,
    instance: WorldInstanceId,
    keys: Vec<SectorKey>,
    generation: GenerationRevision,
    content: WorldContentRevision,
) -> (Vec<SectorTicket>, bool) {
    let mut seen = HashSet::with_capacity(keys.len());
    let desired: Vec<_> = keys.into_iter().filter(|key| seen.insert(*key)).collect();
    let desired_keys: HashSet<_> = desired.iter().copied().collect();
    let mut retained = HashMap::with_capacity(previous.len());
    for ticket in previous {
        let matches = ticket.instance() == instance
            && ticket.generation() == generation
            && ticket.content_revision() == content
            && desired_keys.contains(&ticket.key());
        if matches {
            if let Some(displaced) = retained.insert(ticket.key(), ticket) {
                sectors.unpin(displaced);
            }
        } else {
            sectors.unpin(ticket);
        }
    }

    let mut tickets = Vec::with_capacity(desired.len());
    let mut complete = true;
    for key in desired {
        if let Some(ticket) = retained.remove(&key) {
            tickets.push(ticket);
            continue;
        }
        let Some(ticket) = request_sector(sectors, instance, key, generation, content) else {
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
) -> Option<SectorTicket> {
    match sectors.request(instance, key, generation, content) {
        Ok(ticket) => Some(ticket),
        Err(RequestError::CacheFull) if sectors.evict_oldest() => {
            sectors.request(instance, key, generation, content).ok()
        }
        Err(_) => None,
    }
}

fn install_instance_collision(
    worlds: &mut WorldRegistry,
    sectors: &SectorManager<SectorPayload>,
    instance: WorldInstanceId,
) {
    let mut colliders = Vec::new();
    let mut seen_boxes = HashSet::new();
    let mut surfaces = Vec::new();
    let mut seen_triangles = HashSet::new();
    for (ticket, _, payload) in sectors.installed() {
        if ticket.instance() != instance {
            continue;
        }
        for map_box in &payload.boxes {
            if map_box.solid && seen_boxes.insert(box_geometry_key(*map_box)) {
                colliders.push(Aabb::from_center_size(map_box.center, map_box.size));
            }
        }
        for triangle in &payload.triangles {
            if triangle.solid && seen_triangles.insert(triangle_geometry_key(*triangle)) {
                surfaces.push(*triangle);
            }
        }
    }
    if let Some(owner) = worlds.get_mut(instance) {
        owner.world.replace_streamed_collision(colliders, surfaces);
    }
}

fn box_geometry_key(map_box: MapBox) -> [u32; 6] {
    [
        map_box.center[0].to_bits(),
        map_box.center[1].to_bits(),
        map_box.center[2].to_bits(),
        map_box.size[0].to_bits(),
        map_box.size[1].to_bits(),
        map_box.size[2].to_bits(),
    ]
}

fn triangle_geometry_key(triangle: MapTriangle) -> [u32; 9] {
    let [a, b, c] = triangle.vertices;
    [
        a[0].to_bits(),
        a[1].to_bits(),
        a[2].to_bits(),
        b[0].to_bits(),
        b[1].to_bits(),
        b[2].to_bits(),
        c[0].to_bits(),
        c[1].to_bits(),
        c[2].to_bits(),
    ]
}
