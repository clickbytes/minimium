//! Presentation landforms. No erosion simulation, mutable state or geometry edits.
pub const SURFACE_RECIPE_REVISION: u32 = 2;

#[derive(Clone, Copy, Debug)]
pub struct SurfaceRecipe {
    pub landform_radius_m: f64,
    pub retention_strength: f32,
    pub exposure_strength: f32,
}

impl Default for SurfaceRecipe {
    fn default() -> Self {
        Self {
            landform_radius_m: 256.0,
            retention_strength: 0.65,
            exposure_strength: 0.45,
        }
    }
}

impl SurfaceRecipe {
    pub fn valid(self) -> bool {
        self.landform_radius_m.is_finite()
            && (4.0..=4096.0).contains(&self.landform_radius_m)
            && (0.0..=1.0).contains(&self.retention_strength)
            && (0.0..=1.0).contains(&self.exposure_strength)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Landform {
    pub retention: f32,
    pub exposure: f32,
}

impl Landform {
    pub fn from_heights(center: f64, ring: [f64; 4], slope: f32, recipe: SurfaceRecipe) -> Self {
        let average = ring.iter().sum::<f64>() * 0.25;
        let range = ring.iter().copied().fold(center, f64::max)
            - ring.iter().copied().fold(center, f64::min);
        // The minimum relief suppresses quantisation in almost flat source data.
        let concavity = ((average - center) / range.max(2.0)).clamp(-1.0, 1.0) as f32;
        let steep = (slope / 0.25).clamp(0.0, 1.0);
        Self {
            retention: (0.45 + concavity * recipe.retention_strength - steep * 0.35)
                .clamp(0.0, 1.0),
            exposure: (0.2 - concavity * recipe.exposure_strength + steep * 0.65).clamp(0.0, 1.0),
        }
    }
}
