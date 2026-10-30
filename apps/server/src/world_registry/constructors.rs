use super::*;
use crate::environment::profile_for_manifest;

impl WorldRegistry {
    pub(crate) fn legacy_primary(map: WorldMap) -> Self {
        let definition = WorldDefinitionId::new(1).expect("legacy definition ID is nonzero");
        let instance = WorldInstanceId::new(1).expect("legacy instance ID is nonzero");
        let generation = GenerationSpec::new(
            WorldSeed::new(0),
            GenerationRevision::new(1).expect("legacy generator revision is nonzero"),
            1,
        )
        .expect("legacy position quantum is nonzero");
        let revision = WorldContentRevision::new(1).expect("legacy content revision is nonzero");
        let digest = GeneratorDigest::legacy_map_fingerprint(map.fingerprint());
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
                entities: minimium_core::entity::EntityStore::default(),
                portal: None,
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
            entity_sectors: BTreeMap::new(),
            entity_origins: BTreeMap::new(),
        }
    }

    pub(crate) fn register(
        &mut self,
        instance: WorldInstanceId,
        manifest: WorldManifest,
        map: WorldMap,
    ) -> Result<(), RegistryError> {
        if self.instances.contains_key(&instance) {
            return Err(RegistryError::DuplicateInstance);
        }
        let mut world = World::default();
        world.load_map_streamed(&map);
        world.set_gravity(
            minimium_core::world::GravityPolicy::for_manifest(manifest).character_gravity(),
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
                entities: minimium_core::entity::EntityStore::default(),
                portal: None,
            },
        );
        Ok(())
    }

    pub(crate) fn try_from_generated_worlds(
        worlds: Vec<GeneratedWorld>,
    ) -> Result<Self, RegistryError> {
        let mut worlds = worlds.into_iter();
        let primary = worlds.next().ok_or(RegistryError::EmptyWorlds)?;
        let primary_id = primary.instance;
        let mut registry = Self::from_generated(primary);
        for world in worlds {
            registry.register_generated(world)?;
        }
        registry.primary = primary_id;
        Ok(registry)
    }

    pub(crate) fn from_generated_worlds(worlds: Vec<GeneratedWorld>) -> Self {
        Self::try_from_generated_worlds(worlds)
            .expect("checked generated worlds are nonempty and have unique IDs")
    }

    fn from_generated(world: GeneratedWorld) -> Self {
        let map = Arc::new(world.map);
        let mut simulation = World::default();
        simulation.load_map_streamed(&map);
        simulation.set_gravity(
            minimium_core::world::GravityPolicy::for_manifest(world.manifest).character_gravity(),
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
                entities: minimium_core::entity::EntityStore::default(),
                portal: world.portal,
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
            entity_sectors: BTreeMap::new(),
            entity_origins: BTreeMap::new(),
        }
    }

    fn register_generated(&mut self, world: GeneratedWorld) -> Result<(), RegistryError> {
        let instance = world.instance;
        let portal = world.portal;
        self.register(instance, world.manifest, world.map)?;
        self.instances
            .get_mut(&instance)
            .expect("newly registered instance exists")
            .portal = portal;
        Ok(())
    }
}
