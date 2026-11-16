use super::*;
impl DesktopHost {
    pub(super) fn install_world(
        &mut self,
        owner: super::super::sector_scene::SceneInterestOwner,
        change: crate::application::WorldContextChange,
    ) -> bool {
        let context = change.context;
        let fingerprint = change.map_fingerprint;
        let manifest = change.manifest;
        let instance = context.instance();
        let Some(map) = self
            .application
            .connection_view()
            .session()
            .and_then(|session| session.resolved_world_content_for_context(context))
            .map(|content| content.map().clone())
        else {
            return false;
        };
        if map.fingerprint() != fingerprint {
            return false;
        }
        let target_position = change.position;
        let water = self.water_presentation_for(context);
        let Some(terrain_inputs) = self
            .application
            .connection_view()
            .session()
            .and_then(|session| session.terrain_inputs_for_context(context))
        else {
            return false;
        };
        let scene = match self.sector_cache.ensure_scene(
            owner,
            instance,
            manifest,
            target_position,
            minimium_renderer::terrain_view::TerrainView::new(
                target_position,
                self.fov_degrees.to_radians(),
                self.renderer.size.height,
            ),
            self.renderer.render_origin,
            self.renderer.world_geometry.origin_generation(),
            &self.application.world_catalog_view(),
            &self.application.game_view().world_manifests,
            Some((map.as_ref(), manifest)),
            terrain_inputs,
            water,
        ) {
            Ok(scene) => scene,
            Err(error) => {
                self.application
                    .record_diagnostic_failure(error.code(), &error.to_string());
                self.renderer.release_scene_stage(instance, owner);
                return false;
            }
        };
        let gpu_ready = scene.as_ref().is_some_and(|scene| {
            self.renderer.cache_sector_scene(instance, scene)
                && self
                    .renderer
                    .activate_sector_scene(instance, scene, fingerprint)
        });
        if !gpu_ready {
            self.renderer.release_scene_stage(instance, owner);
        }
        self.application.set_loading(!gpu_ready);
        self.renderer.scene_visible = gpu_ready;
        let previous_context = change.previous_context;
        let previous_instance = change.previous_instance;
        self.renderer.clear_remote_vertices();
        self.gameplay_presentation.clear_remote_players();
        if let Some(scene) = scene.as_ref() {
            if owner == super::super::sector_scene::SceneInterestOwner::Active(context)
                && !self
                    .application
                    .report_active_scene_ready(scene.cpu.clone())
            {
                return false;
            }
        }
        self.last_render_position = if gpu_ready {
            target_position
        } else {
            [f32::INFINITY; 3]
        };
        self.input.yaw = -std::f32::consts::FRAC_PI_2;
        self.input.pitch = 0.0;
        self.application.input.clear_held();
        self.input.jump_pending = false;
        self.renderer.release_scene_stage(
            instance,
            super::super::sector_scene::SceneInterestOwner::Active(context),
        );
        if let Some(source) = previous_context {
            self.sector_cache.promote_interest(
                owner,
                super::super::sector_scene::SceneInterestOwner::Active(context),
            );
            self.sector_cache.release_interest(
                super::super::sector_scene::SceneInterestOwner::Active(source),
            );
            self.renderer.release_scene_stage(
                source.instance(),
                super::super::sector_scene::SceneInterestOwner::Active(source),
            );
        }
        if previous_instance != instance {
            self.release_world_sectors(previous_instance);
        }
        self.metrics.frame_stats.set_gpu_bytes(
            self.renderer.fixed_gpu_bytes
                + u64::from(self.renderer.size.width.max(1))
                    * u64::from(self.renderer.size.height.max(1))
                    * 4,
        );
        true
    }
}

impl DesktopHost {
    pub(super) fn refresh_after_position_change(&mut self) {
        self.renderer.set_render_origin(
            self.application
                .game_view()
                .gameplay
                .prediction
                .character
                .position
                .map(|v| (v / 4096.0).round() * 4096.0),
        );
        self.shadow_state
            .invalidate(super::super::shadows::ShadowInvalidReason::StaleContext);
        self.last_render_position = [f32::INFINITY; 3];
        self.application.set_loading(true);
        self.renderer.scene_visible = false;
        self.application.input.clear_held();
        self.application.status =
            crate::application::ClientStatus::Status(minimium_localization::UiStatus::EarthLoading);
    }
}
