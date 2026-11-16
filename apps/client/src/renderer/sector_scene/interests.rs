use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::renderer) enum SceneInterestOwner {
    Bootstrap(WorldInstanceId),
    Active(WorldContext),
    Pending(TransitionId, WorldContext),
}

impl SceneInterestOwner {
    pub(in crate::renderer) fn instance(self) -> WorldInstanceId {
        match self {
            Self::Bootstrap(instance) => instance,
            Self::Active(context) | Self::Pending(_, context) => context.instance(),
        }
    }

    pub(in crate::renderer) fn geometry_stage(
        self,
    ) -> minimium_renderer::world_geometry_gpu::GeometryStageId {
        let (kind, primary, secondary) = match self {
            Self::Bootstrap(_) => (0, 0_u128, 0_u128),
            Self::Active(context) => (
                1,
                u128::from(context.membership_epoch().get()),
                u128::from(context.content_revision().get()),
            ),
            Self::Pending(transition_id, context) => (
                2,
                transition_id.get(),
                u128::from(context.membership_epoch().get()),
            ),
        };
        minimium_renderer::world_geometry_gpu::GeometryStageId::new(kind, primary, secondary)
    }
}

impl ClientSectorCache {
    pub(super) fn update_terrain_input(
        &mut self,
        instance: WorldInstanceId,
        manifest: WorldManifest,
        terrain: &minimium_procgen::TerrainInputs,
    ) {
        let old = self.terrain_inputs.get(&instance);
        let changed_geometry_source = old.is_some_and(|previous| {
            previous.source().geography().is_some() != terrain.source().geography().is_some()
        });
        let changed_material_profile = old.is_some_and(|previous| {
            previous.source().material_profile() != terrain.source().material_profile()
        });
        let changed_relief_policy = old.is_some_and(|previous| {
            previous.source().relief_policy() != terrain.source().relief_policy()
        });
        let changed = old.is_none_or(|previous| {
            previous.revision() != terrain.revision()
                || changed_geometry_source
                || changed_material_profile
                || changed_relief_policy
        });
        if changed {
            let count = minimium_procgen::planet_sector_count(manifest);
            if let Some(previous) = old {
                self.manager.invalidate_where(instance, |key| {
                    changed_geometry_source
                        || changed_material_profile
                        || changed_relief_policy
                        || previous.sector_version(key, 1, count)
                            != terrain.sector_version(key, 1, count)
                });
            }
            self.presentations
                .retain(|scene_owner, _| scene_owner.instance() != instance);
        }
        self.terrain_inputs
            .insert(instance, Arc::new(terrain.clone()));
    }

    pub(super) fn content_profiles_match(
        inputs: &minimium_procgen::TerrainInputs,
        instance: WorldInstanceId,
        manifest: WorldManifest,
        map: Option<&WorldMap>,
    ) -> bool {
        inputs.source().world() == instance
            && inputs.overlays().world() == Some(instance)
            && map.is_some_and(|map| inputs.source().matches_map(instance, manifest, map))
    }

    pub(super) fn interest_matches(
        &self,
        owner: SceneInterestOwner,
        keys: &[SectorKey],
        manifest: WorldManifest,
    ) -> bool {
        self.interests.get(&owner).is_some_and(|interest| {
            interest.keys == keys
                && interest.tickets.len() == keys.len()
                && interest.tickets.iter().all(|ticket| {
                    self.manager
                        .state(*ticket)
                        .is_some_and(|state| state != minimium_sectors::SectorState::Failed)
                })
                && interest.generation == manifest.generation().revision()
                && interest.content_revision == manifest.content_revision()
        })
    }

    pub(in crate::renderer) fn release_interest(&mut self, owner: SceneInterestOwner) {
        self.presentations.remove(&owner);
        self.water_presentations.remove(&owner);
        let _ = self.worker.cancel(owner);
        self.scene_collision_worker.cancel(owner);
        if let Some(interest) = self.interests.remove(&owner) {
            for ticket in interest.tickets {
                self.manager.unpin(ticket);
                let shared = self
                    .interests
                    .values()
                    .any(|other| other.tickets.contains(&ticket));
                if !shared {
                    let _ = self.manager.cancel(ticket);
                }
            }
        }
    }

    pub(in crate::renderer) fn promote_interest(
        &mut self,
        source: SceneInterestOwner,
        target: SceneInterestOwner,
    ) {
        if source == target {
            return;
        }
        self.release_interest(target);
        if let Some(interest) = self.interests.remove(&source) {
            self.interests.insert(target, interest);
        }
        if let Some(scene) = self.presentations.remove(&source) {
            self.presentations.insert(target, scene);
        }
        if let Some(water) = self.water_presentations.remove(&source) {
            self.water_presentations.insert(target, water);
        }
        self.worker.promote(source, target);
        self.scene_collision_worker.promote(source, target);
    }

    pub(super) fn release_world_interests(&mut self, instance: WorldInstanceId) {
        let owners = self
            .interests
            .keys()
            .copied()
            .filter(|owner| owner.instance() == instance)
            .collect::<Vec<_>>();
        for owner in owners {
            self.release_interest(owner);
        }
        self.terrain_inputs.remove(&instance);
        self.world_sources.remove(&instance);
    }
}
