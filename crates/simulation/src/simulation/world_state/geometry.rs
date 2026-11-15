use super::World;
use crate::simulation::{
    collision_index::CollisionIndex,
    spatial_geometry::{CombatCoveragePlan, CombatRaySectorInterval, SpatialGeometryPublication},
};
use minimium_world::world::{Aabb, SectorKey, WorldMap};
use std::sync::Arc;

impl World {
    pub fn set_precise_positions(&mut self, enabled: bool) {
        self.precise_positions = enabled;
    }

    pub fn combat_coverage_plan(
        &self,
        origin: [f32; 3],
        direction: [f32; 3],
        range: f32,
        required: &[CombatRaySectorInterval],
    ) -> CombatCoveragePlan {
        let ready = required.iter().all(|domain| {
            self.spatial_geometry
                .combat_replacements
                .contains_key(&domain.key)
        });
        CombatCoveragePlan::new(
            Arc::clone(&self.spatial_geometry),
            origin,
            direction,
            range,
            required.to_vec(),
            ready,
        )
    }

    pub fn add_collider(&mut self, collider: Aabb) {
        Arc::make_mut(&mut self.spatial_geometry).with_local_collider(collider);
    }

    pub fn load_map(&mut self, map: &WorldMap) {
        self.spawn = map.spawn();
        self.spawn_mm = map.spawn_position_mm();
        let index = Arc::new(CollisionIndex::from_geometry(
            map.solid_colliders(),
            map.solid_triangles(),
        ));
        self.spatial_geometry = Arc::new(SpatialGeometryPublication::new(
            self.spatial_geometry.revision().saturating_add(1),
            Arc::clone(&index),
            index,
            Default::default(),
        ));
        self.reset_collision_queries();
    }

    /// Loads map metadata while movement collision remains sector-streamed.
    pub fn load_map_streamed(&mut self, map: &WorldMap) {
        self.spawn = map.spawn();
        self.spawn_mm = map.spawn_position_mm();
        let baseline = Arc::new(CollisionIndex::from_geometry(
            map.solid_colliders(),
            map.solid_triangles(),
        ));
        self.spatial_geometry = Arc::new(SpatialGeometryPublication::new(
            self.spatial_geometry.revision().saturating_add(1),
            Arc::new(CollisionIndex::default()),
            baseline,
            Default::default(),
        ));
        self.reset_collision_queries();
    }

    pub fn install_streamed_spatial_geometry(
        &mut self,
        index: Arc<CollisionIndex>,
        replacements: std::collections::BTreeMap<SectorKey, Arc<CollisionIndex>>,
        revision: u64,
    ) {
        self.spatial_geometry = Arc::new(SpatialGeometryPublication::new(
            revision,
            index,
            Arc::clone(&self.spatial_geometry.combat_baseline),
            replacements,
        ));
        self.reset_collision_queries();
    }
}
