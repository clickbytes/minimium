//! Adapter from immutable planet generation to the volumetric terrain read port.
use crate::PlanetTerrain;
use minimium_world::terrain::{valid_position, TerrainMaterial, TerrainRevision, TerrainSample};

pub struct PlanetVolumeBase<'a> {
    terrain: &'a PlanetTerrain,
    soil_depth_m: f64,
}

impl PlanetTerrain {
    pub fn volume_base(&self, soil_depth_m: f64) -> Option<PlanetVolumeBase<'_>> {
        (0.0..=16.0)
            .contains(&soil_depth_m)
            .then_some(PlanetVolumeBase {
                terrain: self,
                soil_depth_m,
            })
    }
}

impl minimium_terrain::BaseTerrain for PlanetVolumeBase<'_> {
    fn sample(&self, position: [f64; 3]) -> Option<TerrainSample> {
        if !valid_position(position) {
            return None;
        }
        let radius = position.iter().map(|v| v * v).sum::<f64>().sqrt();
        if radius < 1.0 {
            return None;
        }
        let direction = position.map(|v| v / radius);
        let surface = self
            .terrain
            .ground_sample(crate::terrain_surface::key(direction));
        let distance_m = radius - surface.height_mm as f64 / 1000.0;
        Some(TerrainSample {
            distance_m,
            material: if distance_m < -self.soil_depth_m {
                TerrainMaterial::Rock
            } else {
                TerrainMaterial::Soil
            },
            revision: TerrainRevision(0),
        })
    }
}
