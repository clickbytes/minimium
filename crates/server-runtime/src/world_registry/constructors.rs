use super::*;
use crate::environment::profile_for_manifest;

impl WorldRegistry {
    pub(crate) fn authored_primary(map: WorldMap) -> Self {
        let definition = WorldDefinitionId::new(1).expect("authored definition ID is nonzero");
        let instance = WorldInstanceId::new(1).expect("authored instance ID is nonzero");
        let generation = GenerationSpec::new(
            WorldSeed::new(0),
            GenerationRevision::new(5).expect("current generator revision is nonzero"),
            1,
        )
        .expect("authored position quantum is nonzero");
        let revision = WorldContentRevision::new(1).expect("authored content revision is nonzero");
        let digest = GeneratorDigest::authored_map_fingerprint(map.fingerprint());
        let manifest = WorldManifest::new(
            definition,
            WorldTopology::Dungeon,
            generation,
            revision,
            digest,
        );
        let mut world = World::default();
        world.load_map_streamed(&map);
        let map = Arc::new(map);
        let mut instances = BTreeMap::new();
        instances.insert(
            instance,
            WorldInstance {
                clock_id: ClockId::MVP,
                environment_profile: profile_for_manifest(manifest),
                manifest,
                map,
                world,
                collision_source_revision: 0,
                collision_installed_revision: 0,
                entities: minimium_world::entity::EntityStore::default(),
                content_initialized: false,
                initial_entities: Vec::new(),
            },
        );
        Self {
            instances,
            memberships: HashMap::new(),
            primary: instance,
            next_epoch: 1,
            transfers: HashMap::new(),
            next_transition: 1,
            sector_states: BTreeMap::new(),
            persisted_totals: super::sector_state::PersistedTotals::default(),
        }
    }

    pub(crate) fn register(
        &mut self,
        instance: WorldInstanceId,
        manifest: WorldManifest,
        map: WorldMap,
    ) -> Result<(), RegistryError> {
        validate_manifest_map(manifest, &map)?;
        if self.instances.contains_key(&instance) {
            return Err(RegistryError::DuplicateInstance);
        }
        if self.instances.len() >= minimium_storage::MAX_SNAPSHOT_WORLDS {
            return Err(RegistryError::WorldLimitExceeded);
        }
        let mut world = World::default();
        world.load_map_streamed(&map);
        world.set_gravity(
            minimium_world::world::GravityPolicy::for_manifest(manifest).character_gravity(),
        );
        world.set_precise_positions(manifest.planet().is_some());
        let map = Arc::new(map);
        let environment_profile = profile_for_manifest(manifest);
        self.instances.insert(
            instance,
            WorldInstance {
                clock_id: ClockId::MVP,
                environment_profile,
                manifest,
                map,
                world,
                collision_source_revision: 0,
                collision_installed_revision: 0,
                entities: minimium_world::entity::EntityStore::default(),
                content_initialized: false,
                initial_entities: Vec::new(),
            },
        );
        Ok(())
    }

    pub(crate) fn try_from_world_definitions(
        worlds: Vec<WorldDefinition>,
    ) -> Result<Self, RegistryError> {
        let mut worlds = worlds.into_iter();
        let primary = worlds.next().ok_or(RegistryError::EmptyWorlds)?;
        validate_manifest_map(primary.manifest, &primary.map)?;
        let primary_id = primary.instance;
        let mut registry = Self::from_definition(primary);
        for world in worlds {
            registry.register_definition(world)?;
        }
        registry.primary = primary_id;
        Ok(registry)
    }

    #[cfg(test)]
    pub(crate) fn from_world_definitions(worlds: Vec<WorldDefinition>) -> Self {
        Self::try_from_world_definitions(worlds)
            .expect("checked generated worlds are nonempty and have unique IDs")
    }

    fn from_definition(world: WorldDefinition) -> Self {
        let map = Arc::new(world.map);
        let mut simulation = World::default();
        simulation.load_map_streamed(&map);
        simulation.set_gravity(
            minimium_world::world::GravityPolicy::for_manifest(world.manifest).character_gravity(),
        );
        simulation.set_precise_positions(world.manifest.planet().is_some());
        let mut instances = BTreeMap::new();
        instances.insert(
            world.instance,
            WorldInstance {
                clock_id: ClockId::MVP,
                environment_profile: profile_for_manifest(world.manifest),
                manifest: world.manifest,
                map,
                world: simulation,
                collision_source_revision: 0,
                collision_installed_revision: 0,
                entities: minimium_world::entity::EntityStore::default(),
                content_initialized: false,
                initial_entities: world.initial_entities,
            },
        );
        Self {
            instances,
            memberships: HashMap::new(),
            primary: world.instance,
            next_epoch: 1,
            transfers: HashMap::new(),
            next_transition: 1,
            sector_states: BTreeMap::new(),
            persisted_totals: super::sector_state::PersistedTotals::default(),
        }
    }

    fn register_definition(&mut self, world: WorldDefinition) -> Result<(), RegistryError> {
        let instance = world.instance;
        let initial_entities = world.initial_entities;
        self.register(instance, world.manifest, world.map)?;
        let owner = self
            .instances
            .get_mut(&instance)
            .expect("newly registered instance exists");
        owner.initial_entities = initial_entities;
        Ok(())
    }
}

fn validate_manifest_map(manifest: WorldManifest, map: &WorldMap) -> Result<(), RegistryError> {
    minimium_procgen::validate_world_manifest(manifest)
        .map_err(|_| RegistryError::InvalidManifest)?;
    if manifest.source() == minimium_world::world::WorldSource::Authored
        && manifest.generator_digest()
            != GeneratorDigest::authored_map_fingerprint(map.fingerprint())
    {
        return Err(RegistryError::InvalidManifest);
    }
    Ok(())
}
