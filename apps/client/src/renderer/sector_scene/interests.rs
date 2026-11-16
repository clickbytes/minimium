use super::*;
pub(in crate::renderer) use minimium_client_runtime::world_readiness::WorldReadinessOwner as SceneInterestOwner;

pub(in crate::renderer) trait SceneInterestPresentation {
    fn geometry_stage(self) -> minimium_renderer::world_geometry_gpu::GeometryStageId;
}

impl SceneInterestPresentation for SceneInterestOwner {
    fn geometry_stage(self) -> minimium_renderer::world_geometry_gpu::GeometryStageId {
        let (kind, primary, secondary) = match self {
            Self::Bootstrap(_) => (0, 0_u128, 0_u128),
            Self::Active(context) => (
                1,
                u128::from(context.membership_epoch().get()),
                u128::from(context.content_revision().get()),
            ),
            Self::Pending(id, context) => {
                (2, id.get(), u128::from(context.membership_epoch().get()))
            }
        };
        minimium_renderer::world_geometry_gpu::GeometryStageId::new(kind, primary, secondary)
    }
}

impl ClientSectorCache {
    pub(in crate::renderer) fn release_interest(&mut self, owner: SceneInterestOwner) {
        self.presentation.presentations.remove(&owner);
        self.presentation.water_presentations.remove(&owner);
        let _ = self.presentation.worker.cancel(owner);
        self.cpu.release_interest(owner);
    }

    pub(in crate::renderer) fn promote_interest(
        &mut self,
        source: SceneInterestOwner,
        target: SceneInterestOwner,
    ) {
        if source == target {
            return;
        }
        if !self.cpu.promote_interest(source, target) {
            return;
        }
        self.presentation.presentations.remove(&target);
        self.presentation.water_presentations.remove(&target);
        let _ = self.presentation.worker.cancel(target);
        if let Some(scene) = self.presentation.presentations.remove(&source) {
            self.presentation.presentations.insert(target, scene);
        }
        if let Some(water) = self.presentation.water_presentations.remove(&source) {
            self.presentation.water_presentations.insert(target, water);
        }
        self.presentation.worker.promote(source, target);
    }

    pub(super) fn release_world_interests(&mut self, instance: WorldInstanceId) {
        let owners: Vec<_> = self
            .presentation
            .presentations
            .keys()
            .chain(self.presentation.water_presentations.keys())
            .copied()
            .filter(|owner| owner.instance() == instance)
            .collect();
        for owner in owners {
            self.release_interest(owner);
        }
        self.cpu.remove_world_data(instance);
    }
}
