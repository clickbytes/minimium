use super::*;
use crate::storage::{WorldSaveEntry, WorldSaveInput};
use std::io;

impl WorldRegistry {
    pub(crate) fn component(
        &self,
        instance: WorldInstanceId,
        entity: EntityId,
        kind: ComponentTypeId,
    ) -> Option<&minimium_world::entity::ComponentBlob> {
        self.instances.get(&instance)?.entities.get(entity, kind)
    }

    pub(crate) fn content_initialized(&self, instance: WorldInstanceId) -> Option<bool> {
        self.instances
            .get(&instance)
            .map(|owner| owner.content_initialized)
    }

    pub(crate) fn component_views(
        &self,
        instance: WorldInstanceId,
    ) -> Option<
        impl Iterator<
                Item = (
                    EntityId,
                    ComponentTypeId,
                    minimium_world::entity::SchemaVersion,
                    &[u8],
                ),
            > + '_,
    > {
        Some(
            self.instances
                .get(&instance)?
                .entities
                .iter_components()
                .map(|(entity, kind, blob)| (entity, kind, blob.schema, blob.bytes.as_slice())),
        )
    }

    pub(crate) fn initial_entities(
        &self,
        instance: WorldInstanceId,
    ) -> Vec<minimium_gameplay::InitialEntityView<'_>> {
        self.instances
            .get(&instance)
            .map_or_else(Vec::new, |owner| {
                owner
                    .initial_entities
                    .iter()
                    .map(|entity| minimium_gameplay::InitialEntityView {
                        sector: entity.sector,
                        components: entity
                            .components
                            .iter()
                            .map(|component| minimium_gameplay::InitialComponentView {
                                kind: component.kind,
                                schema: component.schema,
                                bytes: &component.bytes,
                            })
                            .collect(),
                    })
                    .collect()
            })
    }

    pub(crate) fn set_player_movement_policy(
        &mut self,
        player: PlayerId,
        policy: minimium_simulation::MovementPolicy,
    ) {
        if let Some(instance) = self.for_player_mut(player) {
            instance.world.set_player_movement_policy(player, policy);
        }
    }

    pub(crate) fn snapshot_for_save(
        &self,
        expected_primary_fingerprint: u64,
    ) -> io::Result<WorldSaveInput> {
        let primary = self.primary();
        let primary_map_fingerprint = primary.map.fingerprint();
        if primary_map_fingerprint != expected_primary_fingerprint {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "primary map fingerprint changed while saving",
            ));
        }
        let worlds = self
            .iter()
            .map(|(instance, owner)| WorldSaveEntry {
                instance,
                map_fingerprint: owner.map.fingerprint(),
                manifest: owner.manifest,
                gravity: owner.world.gravity(),
                players: owner.world.snapshot(),
            })
            .collect();
        let entity_counters = self
            .iter()
            .map(|(instance, owner)| (instance, owner.entities.next_id()))
            .collect();
        let world_initialization = self
            .iter()
            .map(|(instance, owner)| (instance, owner.content_initialized))
            .collect();
        let entities = self
            .iter()
            .flat_map(|(instance, owner)| {
                owner.entities.iter().map(move |id| {
                    let components = owner
                        .entities
                        .iter_components()
                        .filter(|(entity, _, _)| *entity == id)
                        .map(|(_, kind, blob)| crate::storage::PersistedComponent {
                            kind,
                            schema: blob.schema,
                            bytes: blob.bytes.clone(),
                        })
                        .collect();
                    crate::storage::PersistedEntity {
                        world: instance,
                        id,
                        components,
                    }
                })
            })
            .collect();
        Ok(WorldSaveInput {
            primary_instance: self.primary_id(),
            primary_map_fingerprint,
            primary_manifest: primary.manifest,
            worlds,
            sector_states: self.sector_states(),
            entity_counters,
            world_initialization,
            entities,
        })
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
}

impl WorldRegistry {
    pub(crate) fn manifest_for_player(&self, id: PlayerId) -> Option<WorldManifest> {
        Some(self.for_player(id)?.manifest)
    }

    pub(crate) fn map_for_instance(&self, instance: WorldInstanceId) -> Option<&WorldMap> {
        self.instances
            .get(&instance)
            .map(|owner| owner.map.as_ref())
    }

    pub(crate) fn map_fingerprint_for_instance(&self, instance: WorldInstanceId) -> Option<u64> {
        self.map_for_instance(instance).map(WorldMap::fingerprint)
    }
}
