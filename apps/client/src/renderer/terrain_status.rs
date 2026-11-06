//! Read-only presentation diagnostics. No sampling or GPU readback on this path.
use super::Renderer;

impl Renderer {
    pub(super) fn terrain_status(&self) -> Vec<String> {
        let instance = self.game.current_instance;
        let scene = self.sector_cache.presentation(instance);
        let relief = self
            .game
            .world_manifests
            .get(&instance)
            .and_then(|m| m.planet())
            .map(|p| p.peak_relief)
            .unwrap_or_default();
        let surfaces = self.gpu.world_geometry.terrain();
        let vertices: u64 = self
            .gpu
            .active_sector_keys
            .iter()
            .filter_map(|(k, l)| surfaces.get(&(instance, *k, *l)))
            .map(|b| u64::from(b.surface_count))
            .sum();
        let ocean_vertices: u64 = self
            .gpu
            .active_sector_keys
            .iter()
            .filter_map(|(k, l)| surfaces.get(&(instance, *k, *l)))
            .map(|b| u64::from(b.ocean_count))
            .sum();
        let buffers: u64 = surfaces
            .values()
            .map(|b| b.surface.size() + b.ocean.size() + b.objects.size())
            .sum();
        vec![
            format!("terrain view={} patches={}/{} triangles={} resident_surface_mib={:.2}",
                if self.overview_open { "overview" } else { "surface" },
                self.gpu.active_sector_keys.len(), self.terrain_view().patch_budget(),
                vertices/3, buffers as f64/1048576.0),
            format!("ocean shape=sea_level_sphere triangles={} shoreline=ground_intersection", ocean_vertices/3),
            format!("terrain peaks={:.3}x exponent={} source_range={}..{}m climate_relief_influence={:.2}",
                relief.multiplier_permille as f64/1000.0, relief.exponent, relief.start_m, relief.full_m,
                self.gpu.terrain_gpu.uniform.climate[3]),
            format!("terrain last_prepare_ms={:.2} source_cache_mib={:.2} upload_limit=8_patches/4MiB recipe={}",
                scene.map_or(0.0,|s| s.build_ms), scene.map_or(0.0,|s| s.cache_bytes as f64/1048576.0),
                minimium_procgen::surface_habitat::SURFACE_RECIPE_REVISION),
            format!("terrain climate={} material_mib={:.2}; debug source: cyan=mapped orange=procedural",
                if self.gpu.terrain_gpu.uniform.cover[2] > 0.0 { "planet+local-history" } else { "preparing-history" },
                self.gpu.terrain_gpu.bytes as f64/1048576.0),
        ]
    }
}
