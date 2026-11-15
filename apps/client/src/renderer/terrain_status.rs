//! Read-only presentation diagnostics. No sampling or GPU readback on this path.
use super::Renderer;

impl Renderer {
    pub(super) fn terrain_status(&self) -> Vec<String> {
        let instance = self.application.game.current_instance;
        let scene = self.sector_cache.presentation(instance);
        let relief = self
            .application
            .game
            .world_manifests
            .get(&instance)
            .and_then(|m| m.planet())
            .map(|p| p.peak_relief)
            .unwrap_or_default();
        let surfaces = self.gpu.world_geometry.terrain();
        let active_keys = self
            .gpu
            .world_geometry
            .publication()
            .map_or(&[][..], |publication| publication.keys.as_slice());
        let vertices: u64 = active_keys
            .iter()
            .filter_map(|(k, l)| surfaces.get(&(instance, *k, *l)))
            .map(|b| u64::from(b.surface_count))
            .sum();
        let ocean_vertices: u64 = active_keys
            .iter()
            .filter_map(|(k, l)| surfaces.get(&(instance, *k, *l)))
            .map(|b| u64::from(b.ocean_count))
            .sum();
        let buffers: u64 = surfaces
            .values()
            .map(|b| b.surface.size() + b.ocean.size() + b.objects.size())
            .sum();
        let environment = self.render_environment_frame.environment_uniform;
        let view_exposure = minimium_renderer::lighting::view_exposure(
            self.render_environment_frame.render_view,
            environment.global_sun_direction,
            environment.ambient_rgb_exposure[3],
        );
        vec![
            format!(
                "terrain debug={:?} (diagnostic colors bypass lighting)",
                self.terrain_debug
            ),
            format!(
                "terrain light atmosphere={} hdr={} base_exposure={:.2} view_exposure={:.2} moon_flux={:.3e}",
                self.gpu.atmosphere_gpu.is_active(),
                self.gpu.hdr_enabled,
                environment.ambient_rgb_exposure[3],
                view_exposure,
                environment.moon_disk_radiance[3]
            ),
            format!(
                "terrain view={} patches={}/{} triangles={} resident_surface_mib={:.2}",
                if self.application.overview().is_open() {
                    "overview"
                } else {
                    "surface"
                },
                active_keys.len(),
                self.terrain_view().patch_budget(),
                vertices / 3,
                buffers as f64 / 1048576.0
            ),
            format!(
                "ocean shape=sea_level_sphere triangles={} shoreline=ground_intersection",
                ocean_vertices / 3
            ),
            format!(
                "terrain peaks={:.3}x exponent={} source_range={}..{}m climate_relief_influence={:.2}",
                relief.multiplier_permille as f64 / 1000.0,
                relief.exponent,
                relief.start_m,
                relief.full_m,
                self.gpu.terrain_gpu.uniform.climate[3]
            ),
            format!(
                "terrain last_prepare_ms={:.2} source_cache_mib={:.2} upload_limit=8_patches/4MiB recipe={}",
                scene.map_or(0.0, |s| s.build_ms),
                scene.map_or(0.0, |s| s.cache_bytes as f64 / 1048576.0),
                minimium_procgen::surface_habitat::SURFACE_RECIPE_REVISION
            ),
            format!(
                "terrain climate={} material_mib={:.2}; debug source: cyan=mapped orange=procedural",
                if self.gpu.terrain_gpu.uniform.cover[2] > 0.0 {
                    "planet+local-history"
                } else {
                    "preparing-history"
                },
                self.gpu.terrain_gpu.bytes as f64 / 1048576.0
            ),
        ]
    }
}
