//! Borrowed current-terrain query adapters supplied to gameplay systems.
use crate::world_registry::WorldRegistry;
use minimium_procgen::{PlanetTerrain, PreparedGeography, TerrainInputs};
use minimium_terrain::{TerrainConfig, TerrainStore};
use minimium_world::{
    terrain::{SurfaceHit, TerrainRead, TerrainSample},
    world::{WorldInstanceId, WorldManifest},
};
use std::{collections::HashMap, sync::Arc};

pub(super) struct CurrentTerrainRead {
    terrain: PlanetTerrain,
    volumes: Arc<TerrainStore>,
    soil_depth_m: f64,
}
pub(super) fn build(
    worlds: &WorldRegistry,
    geography: Option<&Arc<dyn PreparedGeography>>,
    soil_depth_m: f64,
) -> HashMap<WorldInstanceId, CurrentTerrainRead> {
    worlds
        .iter()
        .filter_map(|(instance, owner)| {
            let definition = owner.manifest.planet()?;
            let terrain = planet(owner.manifest, definition, geography, owner)?;
            let volumes = owner.volumes.clone().or_else(|| {
                TerrainStore::new(
                    instance,
                    TerrainConfig {
                        max_chunks: 1,
                        max_edit_radius_m: 0.25,
                    },
                )
                .map(Arc::new)
            })?;
            Some((
                instance,
                CurrentTerrainRead {
                    terrain,
                    volumes,
                    soil_depth_m,
                },
            ))
        })
        .collect()
}

pub(super) fn safe_planet_spawn(
    worlds: &WorldRegistry,
    instance: WorldInstanceId,
    geography: Option<&Arc<dyn PreparedGeography>>,
    soil_depth_m: f64,
) -> Option<minimium_world::world::Position3Mm> {
    let owner = worlds.get(instance)?;
    let definition = owner.manifest.planet()?;
    let origin = owner.map.spawn_position_mm()?.0;
    let length = origin
        .iter()
        .map(|value| (*value as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    if !length.is_finite() || length <= 1.0 {
        return None;
    }
    let direction = origin.map(|value| (value as f64 / length) as f32);
    let inputs = TerrainInputs {
        geography: geography.cloned(),
        edits: Some(owner.terrain.clone()),
        remnants: Some(owner.remnants.clone()),
        volumes: owner.volumes.clone(),
    };
    if minimium_procgen::planet_point_at_direction_with_geography(
        owner.manifest,
        direction,
        inputs.clone(),
    )?
    .ocean
    {
        return None;
    }
    let ground = minimium_procgen::planet_safe_spawn_position_at_direction_with_geography(
        owner.manifest,
        direction,
        inputs,
    )?;
    let ground_radius = ground
        .iter()
        .map(|value| (*value as f64).powi(2))
        .sum::<f64>()
        .sqrt();
    if !ground_radius.is_finite() || ground_radius <= 1.0 {
        return None;
    }
    let feet = std::array::from_fn(|axis| {
        ground[axis] + (ground[axis] as f64 / ground_radius * 2_000.0).round() as i64
    });
    let volumes = owner.volumes.clone().or_else(|| {
        TerrainStore::new(
            instance,
            TerrainConfig {
                max_chunks: 1,
                max_edit_radius_m: 0.25,
            },
        )
        .map(Arc::new)
    })?;
    let terrain = PlanetTerrain::new(
        owner.manifest.generation(),
        minimium_procgen::planet_surface_for(owner.manifest),
        Some(definition),
        TerrainInputs {
            geography: geography.cloned(),
            edits: Some(owner.terrain.clone()),
            remnants: Some(owner.remnants.clone()),
            volumes: Some(volumes.clone()),
        },
    )
    .ok()?;
    let read = CurrentTerrainRead {
        terrain,
        volumes,
        soil_depth_m,
    };
    let position = minimium_world::world::Position3Mm(feet);
    crate::world_registry::terrain_capsule_clear(&read, position).then_some(position)
}

fn planet(
    manifest: WorldManifest,
    definition: minimium_world::world::PlanetDefinition,
    geography: Option<&Arc<dyn PreparedGeography>>,
    owner: &crate::world_registry::WorldInstance,
) -> Option<PlanetTerrain> {
    Some(
        PlanetTerrain::new(
            manifest.generation(),
            minimium_procgen::planet_surface_for(manifest),
            Some(definition),
            TerrainInputs {
                geography: geography.cloned(),
                edits: Some(owner.terrain.clone()),
                remnants: Some(owner.remnants.clone()),
                volumes: owner.volumes.clone(),
            },
        )
        .ok()?,
    )
}

impl TerrainRead for CurrentTerrainRead {
    fn sample(&self, position_m: [f64; 3]) -> Option<TerrainSample> {
        let base = self.terrain.volume_base(self.soil_depth_m)?;
        self.volumes.view(&base).sample(position_m)
    }

    fn surface_hit(
        &self,
        origin_m: [f64; 3],
        direction: [f64; 3],
        reach_m: f64,
    ) -> Option<SurfaceHit> {
        let base = self.terrain.volume_base(self.soil_depth_m)?;
        self.volumes
            .view(&base)
            .surface_hit(origin_m, direction, reach_m)
    }
}
