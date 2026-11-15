use super::*;
use minimium_world::terrain::{SurfaceHit, SurfaceTarget, TerrainRead};

pub(super) struct FirstHit(pub Option<SurfaceHit>);
impl minimium_simulation::tools::ToolScene for FirstHit {
    fn first_hit(&self, _: [f64; 3], _: f64) -> Option<SurfaceHit> {
        self.0
    }
}

pub(super) fn planet(
    manifest: minimium_world::world::WorldManifest,
    inputs: minimium_procgen::TerrainInputs,
) -> Option<minimium_procgen::PlanetTerrain> {
    Some(minimium_procgen::PlanetTerrain::for_manifest(manifest, inputs).ok()?)
}

pub(super) fn edit_error(error: minimium_terrain::TerrainEditError) -> ActionRejection {
    use minimium_terrain::TerrainEditError::*;
    match error {
        Capacity | RevisionExhausted => ActionRejection::Capacity,
        Stale => ActionRejection::Stale,
        NoMaterial => ActionRejection::WrongMaterial,
        _ => ActionRejection::InvalidIntent,
    }
}

pub(super) fn query(
    owner: &crate::world_registry::WorldInstance,
    player: PlayerId,
    store: &TerrainStore,
    base: &dyn minimium_terrain::BaseTerrain,
    direction: [f32; 3],
    reach: f64,
) -> Option<SurfaceHit> {
    let eye = owner.world.character_geometry(player)?.eye_position_m();
    let length = direction
        .iter()
        .map(|v| f64::from(*v).powi(2))
        .sum::<f64>()
        .sqrt();
    if !length.is_finite() || length < 1.0e-8 {
        return None;
    }
    let direction = direction.map(|v| f64::from(v) / length);
    let mut hit = owner
        .world
        .tool_surface_hit(store.world(), player, eye, direction, reach)?;
    if let SurfaceTarget::Terrain(_) = hit.target {
        let behind = std::array::from_fn(|i| hit.position_m[i] - hit.normal[i] * 0.01);
        if let Some(sample) = store.view(base).sample(behind) {
            hit.material = sample.material;
            hit.revision = sample.revision;
        }
    }
    Some(hit)
}
