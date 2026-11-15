//! Presentation interest is independent of the player's collision neighborhood.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TerrainView {
    pub position: [f32; 3],
    pub projected_scale: f32,
    overview_planes: Option<[[f32; 3]; 4]>,
}

impl TerrainView {
    pub fn new(position: [f32; 3], vertical_fov: f32, height: u32) -> Self {
        Self {
            // Quantize direction for stable requests, then restore radial height.
            // Cartesian snapping alone moves a coastal camera +/-17 m vertically,
            // repeatedly changing its horizon and displacing the selected LODs.
            position: stable_position(position),
            projected_scale: height.max(1) as f32
                / (2.0 * (vertical_fov.clamp(0.2, 2.8) * 0.5).tan()),
            overview_planes: None,
        }
    }

    /// Planet-centred overview, including a guard band for asynchronous refreshes.
    /// The mirrored Earth projection has the same symmetric side planes.
    pub fn overview(position: [f32; 3], vertical_fov: f32, size: [u32; 2]) -> Self {
        let mut view = Self::new(position, vertical_fov, size[1]);
        let forward = -glam::Vec3::from_array(view.position).normalize_or_zero();
        let mut right = forward.cross(glam::Vec3::Y).normalize_or_zero();
        if right.length_squared() < 0.5 {
            right = glam::Vec3::X;
        }
        let up = right.cross(forward).normalize_or_zero();
        let vertical = (vertical_fov.clamp(0.2, 2.8) * 0.5).tan() * 1.2;
        let horizontal = vertical * size[0].max(1) as f32 / size[1].max(1) as f32;
        view.overview_planes = Some([
            (forward * horizontal + right).normalize().to_array(),
            (forward * horizontal - right).normalize().to_array(),
            (forward * vertical + up).normalize().to_array(),
            (forward * vertical - up).normalize().to_array(),
        ]);
        view
    }

    pub fn cell_error_limit(self) -> f64 {
        8.0 / f64::from(self.projected_scale.max(1.0))
    }

    pub fn patch_budget(self) -> usize {
        if self.overview_planes.is_some() {
            512
        } else {
            256
        }
    }

    /// Keep the walking neighborhood on one canonical mesh inside the existing
    /// leaf budget. The footprint scales down on small procedural planets.
    pub(crate) fn ground_detail_radius(self, sector_width: f64) -> f64 {
        if self.overview_planes.is_none() {
            (sector_width * 2.0).min(160.0)
        } else {
            0.0
        }
    }

    pub(crate) fn intersects(self, center: [f64; 3], radius: f64) -> bool {
        self.overview_planes.is_none_or(|planes| {
            planes.iter().all(|normal| {
                (0..3)
                    .map(|i| (center[i] - f64::from(self.position[i])) * f64::from(normal[i]))
                    .sum::<f64>()
                    >= -radius
            })
        })
    }

    pub(crate) fn surface_priority(self, surface: super::coast_probe::SurfaceClass) -> f64 {
        use super::coast_probe::SurfaceClass;
        match (self.overview_planes.is_some(), surface) {
            (true, SurfaceClass::Coast) => 8.0,
            (true, SurfaceClass::Land) | (false, SurfaceClass::Coast) => 2.0,
            _ => 1.0,
        }
    }
}

fn stable_position(position: [f32; 3]) -> [f32; 3] {
    let point = glam::DVec3::from_array(position.map(f64::from));
    let snapped = glam::DVec3::from_array(position.map(|v| (f64::from(v) / 20.0).round() * 20.0));
    if snapped.length_squared() < 1.0 {
        return position;
    }
    (snapped.normalize() * point.length().round())
        .as_vec3()
        .to_array()
}
