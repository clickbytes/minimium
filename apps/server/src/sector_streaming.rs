use crate::world_registry::WorldRegistry;
use minimium_core::{
    ids::PlayerId,
    world::{
        Aabb, GenerationRevision, MapBox, MapTriangle, SectorKey, SectorRevision,
        WorldContentRevision, WorldInstanceId,
    },
};
use minimium_sectors::{InstallResult, RequestError, SectorManager, SectorTicket};
use std::{collections::HashSet, sync::Arc};

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
    for tickets in active_tickets.values() {
        for ticket in tickets {
            sectors.unpin(*ticket);
        }
    }
    active_tickets.clear();
    for tickets in transfer_tickets.values() {
        for ticket in tickets {
            sectors.unpin(*ticket);
        }
    }
    transfer_tickets.clear();

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
        let mut tickets = Vec::with_capacity(request.keys.len());
        let mut complete = true;
        for key in request.keys {
            let Some(ticket) = request_sector(
                sectors,
                request.instance,
                key,
                request.generation,
                request.content,
            ) else {
                complete = false;
                continue;
            };
            if sectors.pin(ticket) {
                tickets.push(ticket);
            } else {
                complete = false;
            }
        }
        active_tickets.insert(request.player, tickets.clone());
        readiness.push((request.player, tickets, complete));
    }

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
        let manifest = worlds
            .get(request.instance)
            .map(|instance| instance.manifest);
        let Some(manifest) = manifest else {
            transfer_readiness.push((request.player, Vec::new(), false, request.client_ready));
            continue;
        };
        let mut tickets = Vec::new();
        let mut complete = true;
        for key in minimium_procgen::sector_neighborhood_for(manifest, request.position) {
            let Some(ticket) = request_sector(
                sectors,
                request.instance,
                key,
                request.generation,
                request.content,
            ) else {
                complete = false;
                continue;
            };
            if sectors.pin(ticket) {
                tickets.push(ticket);
            } else {
                complete = false;
            }
        }
        transfer_tickets.insert(request.player, tickets.clone());
        transfer_readiness.push((request.player, tickets, complete, request.client_ready));
    }

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
