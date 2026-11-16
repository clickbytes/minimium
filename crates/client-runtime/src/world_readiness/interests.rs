use super::*;

impl WorldReadiness {
    /// Retires obsolete memberships/transfers in a long-lived host. Sources
    /// without a retained interest are dropped; their late tickets stay stale.
    pub fn retain_interests(
        &mut self,
        owners: &[WorldReadinessOwner],
    ) -> Result<(), WorldReadinessError> {
        if owners.len() > 3 {
            return Err(WorldReadinessError::InvalidRequest);
        }
        let retired: Vec<_> = self
            .interests
            .keys()
            .copied()
            .filter(|owner| !owners.contains(owner))
            .collect();
        for owner in retired {
            self.release_interest(owner);
        }
        self.world_sources
            .retain(|instance, _| owners.iter().any(|owner| owner.instance() == *instance));
        self.terrain_inputs
            .retain(|instance, _| owners.iter().any(|owner| owner.instance() == *instance));
        Ok(())
    }

    pub(super) fn update_terrain_input(
        &mut self,
        instance: WorldInstanceId,
        manifest: WorldManifest,
        terrain: &minimium_procgen::TerrainInputs,
    ) {
        let old = self.terrain_inputs.get(&instance);
        let changed_geometry_source =
            old.is_some_and(|previous| !snapshot::same_source(previous, terrain));
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
            keys.iter().all(|key| interest.keys.contains(key))
                && interest.tickets.len() == interest.keys.len()
                && interest.tickets.iter().all(|ticket| {
                    self.manager
                        .state(*ticket)
                        .is_some_and(|state| state != minimium_sectors::SectorState::Failed)
                })
                && interest.generation == manifest.generation().revision()
                && interest.content_revision == manifest.content_revision()
        })
    }

    pub fn release_interest(&mut self, owner: SceneInterestOwner) {
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

    pub fn promote_interest(
        &mut self,
        source: SceneInterestOwner,
        target: SceneInterestOwner,
    ) -> bool {
        if source == target {
            return true;
        }
        // Promotion changes lifecycle ownership, never the physical source.
        // Pending membership must be the very membership committed by the host.
        let compatible = match (source, target) {
            (SceneInterestOwner::Pending(_, pending), SceneInterestOwner::Active(active)) => {
                pending == active
            }
            (SceneInterestOwner::Bootstrap(instance), SceneInterestOwner::Active(active)) => {
                instance == active.instance()
                    && self
                        .world_sources
                        .get(&instance)
                        .is_some_and(|(_, manifest)| {
                            manifest.content_revision() == active.content_revision()
                        })
            }
            _ => false,
        };
        if !compatible || !self.interests.contains_key(&source) {
            return false;
        }
        self.release_interest(target);
        if let Some(interest) = self.interests.remove(&source) {
            self.interests.insert(target, interest);
        }
        self.scene_collision_worker.promote(source, target);
        true
    }

    pub fn release_world_interests(&mut self, instance: WorldInstanceId) {
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
