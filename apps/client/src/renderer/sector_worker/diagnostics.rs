use super::*;

impl Key {
    pub(super) fn water_summary(
        self,
        owner: SceneInterestOwner,
    ) -> Option<super::super::diagnostics::WaterSceneKey> {
        Some(super::super::diagnostics::WaterSceneKey {
            owner,
            instance: self.world,
            origin_generation: self.origin_generation,
            terrain_version: self.terrain_revision,
            fluid_revision: self.fluid_revision,
            water_level_hash: self.water_level_revision?,
            source_revision: self.water_source_revision?,
            catalog_digest: self.water_catalog_digest?,
            center_m: self.water_center_m?,
            view_position_m: self.view.position,
            interface_cells: self.water_interface_cells?,
        })
    }

    pub(super) fn same_geometry(self, other: Self) -> bool {
        self.world == other.world
            && self.map_fingerprint == other.map_fingerprint
            && self.manifest == other.manifest
            && self.terrain_revision == other.terrain_revision
            && self.material_profile == other.material_profile
            && self.resolved_relief_policy == other.resolved_relief_policy
            && self.origin == other.origin
            && self.origin_generation == other.origin_generation
            && self.water_level_revision.is_some() == other.water_level_revision.is_some()
            && self.view == other.view
    }
}

impl SectorWorker {
    pub(in crate::renderer) fn water_request_summary(
        &self,
        owner: SceneInterestOwner,
    ) -> Option<(super::super::diagnostics::WaterSceneKey, bool)> {
        let state = self.state.0.lock().unwrap();
        let key = *state.requested.get(&owner)?;
        Some((
            key.water_summary(owner)?,
            state.last_failed.get(&owner) == Some(&key),
        ))
    }
}
