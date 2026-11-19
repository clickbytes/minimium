use super::*;

impl PreparedScene {
    pub fn terrain_revision(&self) -> SectorContentVersion {
        self.key.terrain_revision
    }
    pub fn matches(
        &self,
        world: WorldInstanceId,
        manifest: WorldManifest,
        map_fingerprint: u64,
        terrain_revision: SectorContentVersion,
        view: minimium_renderer::terrain_view::TerrainView,
        origin: [f32; 3],
        origin_generation: u64,
        water_level_revision: Option<u64>,
        fluid_revision: u64,
        material_profile: Option<minimium_procgen::ResolvedTerrainMaterialProfile>,
        resolved_relief_policy: Option<minimium_procgen::ResolvedPlanetReliefPolicy>,
    ) -> bool {
        self.key.world == world
            && self.key.map_fingerprint == map_fingerprint
            && self.key.terrain_revision == terrain_revision
            && self.key.manifest == manifest
            && self.key.view == view
            && self.key.origin == origin
            && self.key.origin_generation == origin_generation
            && self.key.water_level_revision == water_level_revision
            && self.key.fluid_revision == fluid_revision
            && self.key.material_profile == material_profile
            && self.key.resolved_relief_policy == resolved_relief_policy
    }
    pub fn geometry_matches(
        &self,
        world: WorldInstanceId,
        manifest: WorldManifest,
        map_fingerprint: u64,
        origin: [f32; 3],
        origin_generation: u64,
        terrain_revision: SectorContentVersion,
        material_profile: Option<minimium_procgen::ResolvedTerrainMaterialProfile>,
        resolved_relief_policy: Option<minimium_procgen::ResolvedPlanetReliefPolicy>,
    ) -> bool {
        self.key.world == world
            && self.key.map_fingerprint == map_fingerprint
            && self.key.manifest == manifest
            && self.key.origin == origin
            && self.key.origin_generation == origin_generation
            && self.key.terrain_revision == terrain_revision
            && self.key.material_profile == material_profile
            && self.key.resolved_relief_policy == resolved_relief_policy
    }
    pub fn position(&self) -> [f32; 3] {
        self.key.view.position
    }
}
