use super::*;

impl WorldRegistry {
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
}
