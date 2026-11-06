use super::{Membership, WorldRegistry};
use minimium_core::ids::PlayerId;
use minimium_protocol::WorldContext;
use minimium_world::world::{
    Position3Mm, TransitionId, WorldInstanceId, WorldLocation, WorldManifest,
};
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(crate) struct TransferOffer {
    pub(crate) transition_id: TransitionId,
    pub(crate) source_context: WorldContext,
    pub(crate) target_context: WorldContext,
    pub(crate) map_fingerprint: u64,
    pub(crate) destination: [f32; 3],
    pub(crate) destination_mm: Position3Mm,
    pub(crate) manifest: WorldManifest,
}

pub(crate) struct CommittedTransfer {
    pub(crate) player: PlayerId,
    pub(crate) transition_id: TransitionId,
    pub(crate) context: WorldContext,
    pub(crate) map_fingerprint: u64,
    pub(crate) destination: [f32; 3],
    pub(crate) destination_mm: Position3Mm,
    pub(crate) source_location: WorldLocation,
    pub(crate) health: u16,
    pub(crate) manifest: WorldManifest,
}

pub(super) struct PendingTransfer {
    offer: TransferOffer,
    ready: bool,
}

impl WorldRegistry {
    pub(crate) fn prepare_portal_transfer(
        &mut self,
        player: PlayerId,
        source_context: WorldContext,
        return_location: Option<WorldLocation>,
    ) -> Option<TransferOffer> {
        if self.transfers.contains_key(&player) || !self.context_matches(player, source_context) {
            return None;
        }
        let source = self.instances.get(&source_context.instance())?;
        let anchor = source.portal?;
        let position = source.world.player(player)?.position;
        if !near_portal(position, anchor.position) {
            return None;
        }
        let target = self.instances.get(&anchor.target_instance)?;
        let target_manifest = target.manifest;
        let target_fingerprint = target.map.fingerprint();
        let return_destination = return_location
            .filter(|location| location.instance() == anchor.target_instance)
            .map(|location| location.position());
        let destination_mm = return_destination.unwrap_or_else(|| {
            if anchor.destination == target.map.spawn() {
                target.map.spawn_position_mm().unwrap_or_else(|| {
                    Position3Mm(
                        anchor
                            .destination
                            .map(|value| (f64::from(value) * 1000.0).round() as i64),
                    )
                })
            } else {
                Position3Mm(
                    anchor
                        .destination
                        .map(|value| (f64::from(value) * 1000.0).round() as i64),
                )
            }
        });
        let destination = destination_mm.0.map(|value| value as f32 / 1000.0);
        if !target
            .map
            .can_place_player_with_gravity(destination, target.world.gravity())
        {
            return None;
        }
        let target_epoch = self.allocate_epoch().ok()?;
        let transition_id = TransitionId::new(self.next_transition)?;
        self.next_transition = self.next_transition.checked_add(1)?;
        let offer = TransferOffer {
            transition_id,
            source_context,
            target_context: WorldContext::new(
                anchor.target_instance,
                target_manifest.content_revision(),
                target_epoch,
            ),
            map_fingerprint: target_fingerprint,
            destination,
            destination_mm,
            manifest: target_manifest,
        };
        self.transfers.insert(
            player,
            PendingTransfer {
                offer,
                ready: false,
            },
        );
        Some(offer)
    }

    pub(crate) fn mark_transfer_ready(
        &mut self,
        player: PlayerId,
        transition_id: TransitionId,
        target_context: WorldContext,
    ) -> bool {
        let Some(source_context) = self
            .transfers
            .get(&player)
            .map(|transfer| transfer.offer.source_context)
        else {
            return false;
        };
        if !self.context_matches(player, source_context) {
            return false;
        }
        let Some(transfer) = self.transfers.get_mut(&player) else {
            return false;
        };
        if transfer.offer.transition_id != transition_id
            || transfer.offer.target_context != target_context
        {
            return false;
        }
        transfer.ready = true;
        true
    }

    pub(crate) fn cancel_transfer(
        &mut self,
        player: PlayerId,
        transition_id: TransitionId,
    ) -> bool {
        if self
            .transfers
            .get(&player)
            .is_some_and(|transfer| transfer.offer.transition_id == transition_id)
        {
            self.transfers.remove(&player);
            true
        } else {
            false
        }
    }

    pub(crate) fn pending_transfer_sector_requests(
        &self,
    ) -> Vec<(PlayerId, WorldInstanceId, [f32; 3], bool)> {
        self.transfers
            .iter()
            .map(|(player, transfer)| {
                (
                    *player,
                    transfer.offer.target_context.instance(),
                    transfer.offer.destination,
                    transfer.ready,
                )
            })
            .collect()
    }

    pub(crate) fn can_map_teleport(&self, id: PlayerId, context: WorldContext) -> bool {
        self.context_matches(id, context) && !self.transfers.contains_key(&id)
    }

    pub(crate) fn commit_ready_transfers(
        &mut self,
        sector_ready: &HashSet<PlayerId>,
    ) -> Vec<CommittedTransfer> {
        let ready_players: Vec<_> = self
            .transfers
            .iter()
            .filter_map(|(player, transfer)| {
                (transfer.ready && sector_ready.contains(player)).then_some(*player)
            })
            .collect();
        let mut committed = Vec::with_capacity(ready_players.len());
        for player in ready_players {
            let Some(transfer) = self.transfers.remove(&player) else {
                continue;
            };
            if let Some(transfer) = self.commit_one_transfer(player, transfer.offer) {
                committed.push(transfer);
            }
        }
        committed
    }

    fn commit_one_transfer(
        &mut self,
        player: PlayerId,
        offer: TransferOffer,
    ) -> Option<CommittedTransfer> {
        let source_instance = offer.source_context.instance();
        let (player_state, source_gravity) = {
            let source = self.instances.get(&source_instance)?;
            let player_state = source.world.player(player)?.clone();
            (player_state, source.world.gravity())
        };
        if let Some(source) = self.instances.get_mut(&source_instance) {
            source.world.leave(player);
        }
        let target = self.instances.get_mut(&offer.target_context.instance())?;
        target.world.join_at_with_health(
            player,
            player_state.name.as_ref().to_owned(),
            offer.destination,
            player_state.health,
        );
        target
            .world
            .restore_position_mm(player, offer.destination_mm);
        self.memberships.insert(
            player,
            Membership {
                instance: offer.target_context.instance(),
                epoch: offer.target_context.membership_epoch(),
            },
        );
        Some(CommittedTransfer {
            player,
            transition_id: offer.transition_id,
            context: offer.target_context,
            map_fingerprint: offer.map_fingerprint,
            destination: offer.destination,
            destination_mm: offer.destination_mm,
            source_location: minimium_storage::world_location::player_location(
                source_instance,
                &player_state,
                source_gravity,
            ),
            health: player_state.health,
            manifest: offer.manifest,
        })
    }
}

fn near_portal(position: [f32; 3], portal: [f32; 3]) -> bool {
    position
        .iter()
        .zip(portal)
        .map(|(position, portal)| (position - portal) * (position - portal))
        .sum::<f32>()
        <= 9.0
}
