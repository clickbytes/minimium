use super::*;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::renderer) enum SceneInterestOwner {
    Bootstrap(WorldInstanceId),
    Active(WorldContext),
    Pending(TransitionId, WorldContext),
}

impl SceneInterestOwner {
    pub(super) fn instance(self) -> WorldInstanceId {
        match self {
            Self::Bootstrap(instance) => instance,
            Self::Active(context) | Self::Pending(_, context) => context.instance(),
        }
    }
}

impl ClientSectorCache {
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
        let _ = self
            .workers
            .get(&owner.instance())
            .is_some_and(|worker| worker.cancel(owner));
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
        if let Some(worker) = self.workers.get_mut(&source.instance()) {
            worker.promote(source, target);
        }
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
        self.workers.remove(&instance);
    }
}
