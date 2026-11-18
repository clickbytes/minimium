//! Read-only presentation diagnostics. No sampling or GPU readback on this path.
use super::DesktopHost;

impl DesktopHost {
    pub(super) fn terrain_status(&self) -> Vec<String> {
        let instance = self.application.core_view().world_instance;
        let scene = self
            .application
            .core_view()
            .context
            .map(super::sector_scene::SceneInterestOwner::Active)
            .or_else(|| Some(super::sector_scene::SceneInterestOwner::Bootstrap(instance)))
            .and_then(|owner| self.sector_cache.presentation(owner));
        let relief = self
            .application
            .world_manifest_catalog_view()
            .get(&instance)
            .and_then(|m| m.planet())
            .map(|p| p.peak_relief)
            .unwrap_or_default();
        let surfaces = self.renderer.world_geometry.terrain();
        let active_keys = self
            .renderer
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
        let rgb_luminance =
            |[r, g, b, _]: [f32; 4]| minimium_renderer::lighting::luminance([r, g, b]);
        let solar = rgb_luminance(environment.global_sun_rgb);
        let lunar_ratio = if solar > 0.0 {
            rgb_luminance(environment.global_moon_rgb) / solar
        } else {
            0.0
        };
        let view = self.render_environment_frame.render_view;
        let base = environment.ambient_rgb_exposure[3];
        let physical = self.renderer.atmosphere_gpu.is_active();
        let (storage, reference) = if physical && self.renderer.hdr_enabled {
            let sun = environment.global_sun_direction;
            (
                minimium_renderer::render_frame::view_exposure(view, sun, base),
                minimium_renderer::render_frame::exposure_reference(view, sun, base),
            )
        } else {
            (base, base)
        };
        vec![
            format!(
                "terrain debug={:?} exposure_ev={} moon_flux_enabled={} lunar_shadow={} utc_ms={}",
                self.terrain_debug,
                self.renderer.fixed_exposure_ev.map_or_else(|| "auto".to_owned(), |ev| ev.to_string()),
                self.moon_flux,
                if !self.renderer.shadow_publish_ready { "unavailable" }
                else if self.prepared_shadow.uniform.flags[1] == 2 { "primary" }
                else if self.prepared_shadow.secondary_moon[0].is_some() { "secondary" } else { "inactive" },
                self.render_environment_frame.sky.as_ref().map_or_else(|| "unavailable".to_owned(), |s| s.utc_ms.get().to_string())
            ),
            format!(
                "terrain light atmosphere={physical} hdr={} base={base:.2} pre_exposure={storage:.2} exposure_reference={reference:.2}",
                self.renderer.hdr_enabled
            ),
            format!(
                "sun_alt={:.1}deg moon_alt={:.1}deg moon_lit={:.0}% ephemeris_flux={:.3e} (relative to full Moon)",
                environment.sun_geometric_world[3].to_degrees(),
                environment.moon_geometric_world[3].to_degrees(),
                100.0 * environment.moon_light_world_phase[3],
                environment.moon_disk_radiance[3],
            ),
            format!("moon/sun_above_atmosphere={lunar_ratio:.3e} moon_disk_above_horizon={:.0}%",
                100.0 * environment.local_moon_rgb_visibility[3]),
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
                self.renderer.terrain_gpu.uniform.climate[3]
            ),
            format!(
                "terrain last_prepare_ms={:.2} source_cache_mib={:.2} upload_limit=8_patches/4MiB recipe={}",
                scene.map_or(0.0, |s| s.build_ms),
                scene.map_or(0.0, |s| s.cache_bytes as f64 / 1048576.0),
                minimium_procgen::surface_habitat::SURFACE_RECIPE_REVISION
            ),
            format!(
                "terrain climate={} material_mib={:.2}; debug source: cyan=mapped orange=procedural",
                if self.renderer.terrain_gpu.uniform.cover[2] > 0.0 {
                    "planet+local-history"
                } else {
                    "preparing-history"
                },
                self.renderer.terrain_gpu.bytes as f64 / 1048576.0
            ),
        ]
    }
}
