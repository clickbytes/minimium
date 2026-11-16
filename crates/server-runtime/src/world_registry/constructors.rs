use super::*;
use crate::environment::profile_for_manifest;

impl WorldRegistry {
    pub(crate) fn authored_primary(
        map: WorldMap,
        material_profile: minimium_procgen::TerrainMaterialProfile,
    ) -> Self {
        let definition = WorldDefinitionId::new(1).expect("authored definition ID is nonzero");
        let instance = WorldInstanceId::new(1).expect("authored instance ID is nonzero");
        let generation = GenerationSpec::new(
            WorldSeed::new(0),
            GenerationRevision::new(6).expect("current generator revision is nonzero"),
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
        let terrain_material_profile = material_profile.bind(manifest, map.fingerprint());
        let mut world = World::default();
        world.load_map_streamed(&map);
        let map = Arc::new(map);
        let mut instances = BTreeMap::new();
        instances.insert(
            instance,
            WorldInstance {
                clock_id: ClockId::MVP,
                environment_profile: profile_for_manifest(manifest),
                collision_tickets: Default::default(),
                volumes: None,
                terrain: Arc::new(minimium_terrain::TerrainSnapshot::empty(10000.0)),
                remnants: Arc::new(minimium_world::remnants::RemnantSnapshot::empty(10000.0)),
                manifest,
                terrain_material_profile,
                resolved_relief_policy: None,
                map,
                world,
                collision_source_revision: 0,
                collision_installed_revision: 0,
                entities: minimium_world::entity::EntityStore::default(),
                content_initialized: false,
                initial_entities: Vec::new(),
            },
        );
        let world_membership_revision = instances.len() as u128;
        Self {
            instances,
            memberships: HashMap::new(),
            primary: instance,
            next_epoch: 1,
            transfers: HashMap::new(),
            next_transition: 1,
            sector_states: BTreeMap::new(),
            persisted_totals: super::sector_state::PersistedTotals::default(),
            player_membership_revision: 0,
            read_identity: std::sync::Arc::new(()),
            world_membership_revision,
            entity_membership_revision: 0,
        }
    }

    pub(crate) fn register(
        &mut self,
        instance: WorldInstanceId,
        manifest: WorldManifest,
        map: WorldMap,
        terrain_material_profile: minimium_procgen::ResolvedTerrainMaterialProfile,
        resolved_relief_policy: Option<minimium_procgen::ResolvedPlanetReliefPolicy>,
    ) -> Result<(), RegistryError> {
        validate_manifest_map(manifest, &map)?;
        if !terrain_material_profile.matches(manifest, map.fingerprint()) {
            return Err(RegistryError::InvalidTerrainMaterialProfile);
        }
        validate_relief_policy(manifest, &map, resolved_relief_policy)?;
        if let Some(owner) = self.instances.values().find(|owner| {
            owner.manifest == manifest && owner.map.fingerprint() == map.fingerprint()
        }) {
            if owner.terrain_material_profile != terrain_material_profile {
                return Err(RegistryError::InvalidTerrainMaterialProfile);
            }
            if owner.resolved_relief_policy != resolved_relief_policy {
                return Err(RegistryError::InvalidReliefPolicy);
            }
        }
        if self.instances.contains_key(&instance) {
            return Err(RegistryError::DuplicateInstance);
        }
        if self.instances.len() >= minimium_storage::MAX_SNAPSHOT_WORLDS {
            return Err(RegistryError::WorldLimitExceeded);
        }
        let next_membership_revision = self
            .world_membership_revision
            .checked_add(1)
            .ok_or(RegistryError::MembershipRevisionExhausted)?;
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
                collision_tickets: Default::default(),
                volumes: None,
                terrain: Arc::new(minimium_terrain::TerrainSnapshot::empty(
                    manifest
                        .planet()
                        .map_or(10000.0, |p| p.radius_mm as f64 / 1000.0),
                )),
                remnants: Arc::new(minimium_world::remnants::RemnantSnapshot::empty(
                    manifest
                        .planet()
                        .map_or(10000.0, |p| p.radius_mm as f64 / 1000.0),
                )),
                manifest,
                terrain_material_profile,
                resolved_relief_policy,
                map,
                world,
                collision_source_revision: 0,
                collision_installed_revision: 0,
                entities: minimium_world::entity::EntityStore::default(),
                content_initialized: false,
                initial_entities: Vec::new(),
            },
        );
        self.world_membership_revision = next_membership_revision;
        Ok(())
    }

    pub(crate) fn try_from_world_definitions(
        worlds: Vec<WorldDefinition>,
    ) -> Result<Self, RegistryError> {
        let mut worlds = worlds.into_iter();
        let primary = worlds.next().ok_or(RegistryError::EmptyWorlds)?;
        validate_manifest_map(primary.manifest, &primary.map)?;
        if !primary
            .terrain_material_profile
            .matches(primary.manifest, primary.map.fingerprint())
        {
            return Err(RegistryError::InvalidTerrainMaterialProfile);
        }
        validate_relief_policy(
            primary.manifest,
            &primary.map,
            primary.resolved_relief_policy,
        )?;
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
                collision_tickets: Default::default(),
                volumes: None,
                terrain: Arc::new(minimium_terrain::TerrainSnapshot::empty(
                    world
                        .manifest
                        .planet()
                        .map_or(10000.0, |p| p.radius_mm as f64 / 1000.0),
                )),
                remnants: Arc::new(minimium_world::remnants::RemnantSnapshot::empty(
                    world
                        .manifest
                        .planet()
                        .map_or(10000.0, |p| p.radius_mm as f64 / 1000.0),
                )),
                manifest: world.manifest,
                terrain_material_profile: world.terrain_material_profile,
                resolved_relief_policy: world.resolved_relief_policy,
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
            player_membership_revision: 0,
            read_identity: std::sync::Arc::new(()),
            world_membership_revision: 1,
            entity_membership_revision: 0,
        }
    }

    fn register_definition(&mut self, world: WorldDefinition) -> Result<(), RegistryError> {
        let instance = world.instance;
        let initial_entities = world.initial_entities;
        self.register(
            instance,
            world.manifest,
            world.map,
            world.terrain_material_profile,
            world.resolved_relief_policy,
        )?;
        let owner = self
            .instances
            .get_mut(&instance)
            .expect("newly registered instance exists");
        owner.initial_entities = initial_entities;
        Ok(())
    }
}

fn validate_relief_policy(
    manifest: WorldManifest,
    map: &WorldMap,
    policy: Option<minimium_procgen::ResolvedPlanetReliefPolicy>,
) -> Result<(), RegistryError> {
    let required = manifest.source() == minimium_world::world::WorldSource::Generated
        && manifest.topology() == WorldTopology::Planet;
    match policy {
        Some(policy) if !policy.matches(manifest, map.fingerprint()) => {
            Err(RegistryError::InvalidReliefPolicy)
        }
        None if required => Err(RegistryError::InvalidReliefPolicy),
        _ => Ok(()),
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
