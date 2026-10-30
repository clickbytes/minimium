use super::*;

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
                manifest,
                map,
                world,
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
        world.set_gravity(gravity_for(manifest.topology()));
        world.set_precise_positions(manifest.planet().is_some());
        let map = Arc::new(map);
        self.instances.insert(
            instance,
            WorldInstance {
                manifest,
                map,
                world,
                portal: None,
            },
        );
        Ok(())
    }

    pub(crate) fn from_generated_worlds(mut worlds: Vec<GeneratedWorld>) -> Self {
        let primary = worlds.remove(0);
        let primary_id = primary.instance;
        let mut registry = Self::from_generated(primary);
        for world in worlds {
            registry
                .register_generated(world)
                .expect("generated world instance IDs are unique");
        }
        registry.primary = primary_id;
        registry
    }

    fn from_generated(world: GeneratedWorld) -> Self {
        let map = Arc::new(world.map);
        let mut simulation = World::default();
        simulation.load_map_streamed(&map);
        simulation.set_gravity(gravity_for(world.manifest.topology()));
        simulation.set_precise_positions(world.manifest.planet().is_some());
        let mut instances = BTreeMap::new();
        instances.insert(
            world.instance,
            WorldInstance {
                manifest: world.manifest,
                map,
                world: simulation,
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
