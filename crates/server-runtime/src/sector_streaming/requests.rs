use super::{PlayerSectorRequest, TransferSectorRequest};
use crate::world_registry::WorldRegistry;

pub(super) fn collect_active_requests(worlds: &WorldRegistry) -> Vec<PlayerSectorRequest> {
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
    requests
}

pub(super) fn collect_transfer_requests(worlds: &WorldRegistry) -> Vec<TransferSectorRequest> {
    let mut requests: Vec<_> = worlds
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
    requests.sort_unstable_by_key(|request| request.player);
    requests
}
