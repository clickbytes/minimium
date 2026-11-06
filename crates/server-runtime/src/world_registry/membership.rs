use super::{RegistryError, WorldRegistry};
use minimium_core::ids::PlayerId;
use minimium_protocol::WorldContext;
use minimium_world::world::{MembershipEpoch, WorldInstanceId};

#[derive(Clone, Copy)]
pub(super) struct Membership {
    pub(super) instance: WorldInstanceId,
    pub(super) epoch: MembershipEpoch,
}

impl WorldRegistry {
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

    pub(crate) fn join_at_precise(
        &mut self,
        instance: WorldInstanceId,
        id: PlayerId,
        name: String,
        position: [f32; 3],
        position_mm: minimium_world::world::Position3Mm,
        health: u16,
    ) -> Result<(), RegistryError> {
        self.join_at(instance, id, name, position, health)?;
        if let Some(owner) = self.instances.get_mut(&instance) {
            owner.world.restore_position_mm(id, position_mm);
        }
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

    pub(super) fn allocate_epoch(&mut self) -> Result<MembershipEpoch, RegistryError> {
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

    pub(crate) fn instance_for_player(&self, id: PlayerId) -> Option<WorldInstanceId> {
        self.memberships
            .get(&id)
            .map(|membership| membership.instance)
    }

    pub(crate) fn world_input_context_is_current(
        &self,
        id: PlayerId,
        context: WorldContext,
    ) -> bool {
        self.context_matches(id, context)
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
