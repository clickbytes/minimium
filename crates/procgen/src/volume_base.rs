//! Adapter from immutable planet generation to the volumetric terrain read port.
use crate::PlanetTerrain;
use minimium_world::terrain::{valid_position, TerrainMaterial, TerrainRevision, TerrainSample};

pub struct PlanetVolumeBase<'a> {
    terrain: &'a PlanetTerrain,
    soil_depth_mm: u16,
}

impl PlanetTerrain {
    pub fn volume_base(&self) -> Option<PlanetVolumeBase<'_>> {
        let profile = self.material_profile()?;
        if self.material_manifest != Some(profile.manifest()) {
            return None;
        }
        Some(PlanetVolumeBase {
            terrain: self,
            soil_depth_mm: profile.profile().soil_depth_mm(),
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
            .ground_sample(crate::terrain_surface::key(direction))
            .ok()?;
        let distance_m = radius - surface.height_mm as f64 / 1000.0;
        Some(TerrainSample {
            distance_m,
            material: if distance_m < -(f64::from(self.soil_depth_mm) / 1000.0) {
                TerrainMaterial::Rock
            } else {
                TerrainMaterial::Soil
            },
            revision: TerrainRevision(0),
        })
    }
}
