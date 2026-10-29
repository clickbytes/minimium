use minimium_core::world::WorldMap;
use minimium_core::{
    ids::PlayerId,
    world::{
        GenerationRevision, GenerationSpec, GeneratorDigest, MembershipEpoch, Position3Mm,
        TransitionId, World, WorldContentRevision, WorldDefinitionId, WorldInstanceId,
        WorldLocation, WorldManifest, WorldOrientationQ30, WorldSectorState, WorldSeed,
        WorldTopology,
    },
};
use minimium_procgen::{DemoPortal, GeneratedWorld};
use minimium_protocol::WorldContext;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

mod constructors;
mod sector_state;

#[cfg(test)]
mod tests;

pub(crate) struct WorldInstance {
    pub(crate) manifest: WorldManifest,
    pub(crate) map: Arc<WorldMap>,
    pub(crate) world: World,
    pub(crate) portal: Option<DemoPortal>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegistryError {
    DuplicateInstance,
    UnknownInstance,
    EpochSpaceExhausted,
    InvalidSectorState,
    StaleSectorRevision,
    SectorStateCapacity,
}

/// Server-owned collection of isolated simulation owners and memberships.
pub(crate) struct WorldRegistry {
    instances: BTreeMap<WorldInstanceId, WorldInstance>,
    memberships: HashMap<PlayerId, Membership>,
    primary: WorldInstanceId,
    next_epoch: u64,
    transfers: HashMap<PlayerId, PendingTransfer>,
    next_transition: u128,
    sector_states: BTreeMap<(WorldInstanceId, minimium_core::world::SectorKey), WorldSectorState>,
}

#[derive(Clone, Copy)]
struct Membership {
    instance: WorldInstanceId,
    epoch: MembershipEpoch,
}

#[derive(Clone, Copy)]
pub(crate) struct TransferOffer {
    pub(crate) transition_id: TransitionId,
    pub(crate) source_context: WorldContext,
    pub(crate) target_context: WorldContext,
    pub(crate) map_fingerprint: u64,
    pub(crate) destination: [f32; 3],
    pub(crate) manifest: WorldManifest,
}

pub(crate) struct CommittedTransfer {
    pub(crate) player: PlayerId,
    pub(crate) transition_id: TransitionId,
    pub(crate) context: WorldContext,
    pub(crate) map_fingerprint: u64,
    pub(crate) destination: [f32; 3],
    pub(crate) source_location: WorldLocation,
    pub(crate) health: u16,
    pub(crate) manifest: WorldManifest,
}

struct PendingTransfer {
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
        let target_epoch = self.allocate_epoch().ok()?;
        let transition_id = TransitionId::new(self.next_transition)?;
        self.next_transition = self.next_transition.checked_add(1)?;
        let destination = return_location
            .filter(|location| location.instance() == anchor.target_instance)
            .map(|location| {
                location
                    .position()
                    .coordinates()
                    .map(|value| value as f32 / 1000.0)
            })
            .unwrap_or(anchor.destination);
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
            let offer = transfer.offer;
            let source_instance = offer.source_context.instance();
            let Some(player_state) = self
                .instances
                .get(&source_instance)
                .and_then(|source| source.world.player(player))
                .cloned()
            else {
                continue;
            };
            let source_gravity = self
                .instances
                .get(&source_instance)
                .map(|source| source.world.gravity())
                .unwrap_or_default();
            if let Some(source) = self.instances.get_mut(&source_instance) {
                source.world.leave(player);
            }
            let Some(target) = self.instances.get_mut(&offer.target_context.instance()) else {
                continue;
            };
            target.world.join_at_with_health(
                player,
                player_state.name.as_ref().to_owned(),
                offer.destination,
                player_state.health,
            );
            self.memberships.insert(
                player,
                Membership {
                    instance: offer.target_context.instance(),
                    epoch: offer.target_context.membership_epoch(),
                },
            );
            committed.push(CommittedTransfer {
                player,
                transition_id: offer.transition_id,
                context: offer.target_context,
                map_fingerprint: offer.map_fingerprint,
                destination: offer.destination,
                source_location: WorldLocation::new(
                    source_instance,
                    Position3Mm(
                        player_state
                            .position
                            .map(|value| (f64::from(value) * 1000.0).round() as i64),
                    ),
                    orientation_for_gravity(source_gravity, player_state.position),
                ),
                health: player_state.health,
                manifest: offer.manifest,
            });
        }
        committed
    }

    pub(crate) fn join_at(
        &mut self,
        instance: WorldInstanceId,
        id: PlayerId,
        name: String,
        position: [f32; 3],
        health: u16,
    ) -> Result<(), RegistryError> {
        if !self.instances.contains_key(&instance) {
            return Err(RegistryError::UnknownInstance);
        }
        if self
            .memberships
            .get(&id)
            .is_none_or(|member| member.instance != instance)
        {
            let epoch = self.allocate_epoch()?;
            if let Some(previous) = self.memberships.insert(id, Membership { instance, epoch }) {
                if previous.instance != instance {
                    if let Some(previous) = self.instances.get_mut(&previous.instance) {
                        previous.world.leave(id);
                    }
                }
            }
        }
        let target = self
            .instances
            .get_mut(&instance)
            .ok_or(RegistryError::UnknownInstance)?;
        target.world.join_at_with_health(id, name, position, health);
        Ok(())
    }

    pub(crate) fn reserve_membership(
        &mut self,
        instance: WorldInstanceId,
        id: PlayerId,
    ) -> Result<WorldContext, RegistryError> {
        let manifest = self
            .instances
            .get(&instance)
            .ok_or(RegistryError::UnknownInstance)?
            .manifest;
        let membership = if let Some(current) = self.memberships.get(&id).copied() {
            if current.instance == instance {
                current
            } else {
                let next = Membership {
                    instance,
                    epoch: self.allocate_epoch()?,
                };
                self.memberships.insert(id, next);
                if let Some(previous) = self.instances.get_mut(&current.instance) {
                    previous.world.leave(id);
                }
                next
            }
        } else {
            let next = Membership {
                instance,
                epoch: self.allocate_epoch()?,
            };
            self.memberships.insert(id, next);
            next
        };
        Ok(WorldContext::new(
            instance,
            manifest.content_revision(),
            membership.epoch,
        ))
    }

    fn allocate_epoch(&mut self) -> Result<MembershipEpoch, RegistryError> {
        let value = self.next_epoch;
        self.next_epoch = value
            .checked_add(1)
            .ok_or(RegistryError::EpochSpaceExhausted)?;
        MembershipEpoch::new(value).ok_or(RegistryError::EpochSpaceExhausted)
    }

    pub(crate) fn context_for_player(&self, id: PlayerId) -> Option<WorldContext> {
        let membership = *self.memberships.get(&id)?;
        let manifest = self.instances.get(&membership.instance)?.manifest;
        Some(WorldContext::new(
            membership.instance,
            manifest.content_revision(),
            membership.epoch,
        ))
    }

    pub(crate) fn context_matches(&self, id: PlayerId, context: WorldContext) -> bool {
        self.context_for_player(id) == Some(context)
    }

    pub(crate) fn manifest_for_player(&self, id: PlayerId) -> Option<WorldManifest> {
        Some(self.for_player(id)?.manifest)
    }

    pub(crate) fn instance_for_player(&self, id: PlayerId) -> Option<WorldInstanceId> {
        self.memberships
            .get(&id)
            .map(|membership| membership.instance)
    }

    pub(crate) fn primary_id(&self) -> WorldInstanceId {
        self.primary
    }

    pub(crate) fn primary(&self) -> &WorldInstance {
        self.instances
            .get(&self.primary)
            .expect("primary world remains registered")
    }

    pub(crate) fn get(&self, instance: WorldInstanceId) -> Option<&WorldInstance> {
        self.instances.get(&instance)
    }

    pub(crate) fn get_mut(&mut self, instance: WorldInstanceId) -> Option<&mut WorldInstance> {
        self.instances.get_mut(&instance)
    }

    pub(crate) fn for_player(&self, id: PlayerId) -> Option<&WorldInstance> {
        self.instances.get(&self.instance_for_player(id)?)
    }

    pub(crate) fn for_player_mut(&mut self, id: PlayerId) -> Option<&mut WorldInstance> {
        let instance = self.instance_for_player(id)?;
        self.instances.get_mut(&instance)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (WorldInstanceId, &WorldInstance)> {
        self.instances.iter().map(|(id, world)| (*id, world))
    }

    pub(crate) fn iter_mut(
        &mut self,
    ) -> impl Iterator<Item = (WorldInstanceId, &mut WorldInstance)> {
        self.instances.iter_mut().map(|(id, world)| (*id, world))
    }

    pub(crate) fn world_input_context_is_current(
        &self,
        id: PlayerId,
        context: WorldContext,
    ) -> bool {
        self.context_matches(id, context)
    }

    pub(crate) fn map_for_instance(&self, instance: WorldInstanceId) -> Option<&WorldMap> {
        self.instances
            .get(&instance)
            .map(|owner| owner.map.as_ref())
    }

    pub(crate) fn map_fingerprint_for_instance(&self, instance: WorldInstanceId) -> Option<u64> {
        self.map_for_instance(instance).map(WorldMap::fingerprint)
    }

    pub(crate) fn leave(&mut self, id: PlayerId) -> bool {
        self.transfers.remove(&id);
        let Some(membership) = self.memberships.remove(&id) else {
            return false;
        };
        if let Some(owner) = self.instances.get_mut(&membership.instance) {
            owner.world.leave(id);
            true
        } else {
            false
        }
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

fn gravity_for(topology: WorldTopology) -> minimium_core::world::CharacterGravity {
    match topology {
        WorldTopology::Planet => {
            minimium_core::world::CharacterGravity::Radial { center: [0.0; 3] }
        }
        WorldTopology::Dungeon => minimium_core::world::CharacterGravity::FixedY,
    }
}

fn orientation_for_gravity(
    gravity: minimium_core::world::CharacterGravity,
    position: [f32; 3],
) -> WorldOrientationQ30 {
    let up = minimium_core::world::character_frame(gravity, position).0;
    let denominator = (2.0 * (1.0 + up[1])).sqrt();
    let quaternion = if denominator <= 1.0e-6 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [
            ((1.0 + up[1]) * 0.5).sqrt(),
            up[2] / denominator,
            0.0,
            -up[0] / denominator,
        ]
    };
    let fixed = quaternion.map(|value| (f64::from(value) * (1_i64 << 30) as f64).round() as i32);
    WorldOrientationQ30::new(fixed).unwrap_or(WorldOrientationQ30::IDENTITY)
}
