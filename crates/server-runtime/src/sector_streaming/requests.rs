use super::{CombatSectorDemand, PlayerSectorRequest, TransferSectorRequest};
use crate::world_registry::WorldRegistry;
use minimium_core::ids::PlayerId;
use std::collections::HashMap;

pub(super) fn collect_active_requests(worlds: &WorldRegistry) -> Vec<PlayerSectorRequest> {
    let mut requests = Vec::new();
    for (instance_id, instance) in worlds.iter() {
        let manifest = instance.manifest;
        let mut players = instance.world.snapshot();
        players.sort_unstable_by_key(|player| player.id);
        requests.extend(players.into_iter().map(|player| {
            let keys = minimium_procgen::sector_neighborhood_for(manifest, player.position);
            PlayerSectorRequest {
                player: player.id,
                instance: instance_id,
                keys,
                generation: manifest.generation().revision(),
                content: manifest.content_revision(),
            }
        }));
    }
    requests.sort_unstable_by_key(|request| request.player);
    requests
}

pub(super) fn collect_combat_requests(
    worlds: &WorldRegistry,
    combat_demands: &mut HashMap<PlayerId, CombatSectorDemand>,
) -> Vec<PlayerSectorRequest> {
    combat_demands
        .retain(|player, demand| worlds.world_input_context_is_current(*player, demand.context));
    combat_demands
        .iter()
        .filter_map(|(player, demand)| {
            let instance = demand.context.instance();
            let manifest = worlds.get(instance)?.manifest;
            let mut keys = demand.keys.clone();
            keys.sort_unstable();
            keys.dedup();
            Some(PlayerSectorRequest {
                player: *player,
                instance,
                keys,
                generation: manifest.generation().revision(),
                content: manifest.content_revision(),
            })
        })
        .collect()
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
