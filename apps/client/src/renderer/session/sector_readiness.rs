use super::*;

impl Renderer {
    pub(in crate::renderer) fn water_presentation_for(
        &self,
        context: minimium_protocol::WorldContext,
    ) -> Option<super::super::sector_worker::WaterPresentation> {
        let session = self.application.connection.session()?;
        if !session.shares_active_world_state(context) {
            return None;
        }
        let catalog = session.water_catalog_for_context(context)?;
        let snapshot = session.fluid_snapshot()?;
        (snapshot.world() == context.instance())
            .then(|| super::super::sector_worker::WaterPresentation::new(catalog, snapshot))
    }

    pub(in crate::renderer) fn refresh_active_sector_scene(&mut self) -> bool {
        if self
            .application
            .connection
            .session()
            .is_some_and(|s| !s.impact_ready() || !s.excavation_ready())
        {
            self.application.game.gameplay.sectors_ready = false;
            return false;
        }
        let Some(manifest) = self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
            .copied()
        else {
            self.application.game.gameplay.sectors_ready = false;
            return false;
        };
        if manifest.planet().is_some() {
            let prediction = &self.application.game.gameplay.prediction;
            let feet = prediction
                .position_mm
                .map_or(prediction.character.position.map(f64::from), |position| {
                    position.0.map(|v| v as f64 / 1000.0)
                });
            let up = minimium_world::world::character_frame(
                prediction.character.gravity,
                prediction.character.position,
            )
            .0
            .map(f64::from);
            if !self
                .application
                .connection
                .session()
                .is_some_and(|session| session.water_region_covers_capsule(feet, up, 1.7))
            {
                self.application.game.gameplay.sectors_ready = false;
                return false;
            }
        }
        let view = self.terrain_view();
        let water = self
            .application
            .game
            .session_context()
            .and_then(|context| self.water_presentation_for(context));
        let scene = match self.sector_cache.ensure_scene(
            self.application.game.session_context().map_or_else(
                || {
                    super::super::sector_scene::SceneInterestOwner::Bootstrap(
                        self.application.game.current_instance,
                    )
                },
                super::super::sector_scene::SceneInterestOwner::Active,
            ),
            self.application.game.current_instance,
            manifest,
            self.application.game.gameplay.prediction.character.position,
            view,
            self.gpu.render_origin,
            self.gpu.world_geometry.origin_generation(),
            &self.application.world_catalog,
            &self.application.game.world_manifests,
            None,
            self.application
                .connection
                .session()
                .map(|session| session.terrain_inputs())
                .unwrap_or_default(),
            water,
        ) {
            Ok(Some(scene)) => scene,
            Ok(None) => {
                self.application.game.gameplay.sectors_ready = false;
                return false;
            }
            Err(error) => {
                self.application
                    .connection
                    .record_diagnostic_failure(error.code(), &error.to_string());
                self.application.game.gameplay.sectors_ready = false;
                return false;
            }
        };
        if manifest.planet().is_some() && scene.prepared.is_none() {
            self.application.game.gameplay.sectors_ready = false;
            return false;
        }
        if !self
            .gpu
            .cache_sector_scene(self.application.game.current_instance, &scene)
            || !self.gpu.activate_sector_scene(
                self.application.game.current_instance,
                &scene,
                self.application
                    .world_catalog
                    .get(&self.application.game.current_instance)
                    .map_or(0, |map| map.fingerprint()),
            )
        {
            let current = self.application.game.session_context().map(|context| {
                (
                    context,
                    self.application
                        .connection
                        .session()
                        .map_or(minimium_procgen::SectorContentVersion::default(), |s| {
                            s.terrain_inputs().revision()
                        }),
                )
            });
            if self.impacts.published_terrain != current {
                self.application.game.gameplay.sectors_ready = false;
                return false;
            }
            self.application.game.gameplay.collision_index = scene.collision;
            self.application.game.gameplay.collision_revision = scene.collision_revision;
            self.last_render_position = [f32::INFINITY; 3];
            self.application.game.gameplay.sectors_ready = true;
            return true;
        }
        if let (Some(context), Some(prepared)) = (
            self.application.game.session_context(),
            scene.prepared.as_ref(),
        ) {
            self.impacts.published_terrain = Some((context, prepared.terrain_revision()));
            self.impacts.published_craters = self
                .application
                .connection
                .session()
                .and_then(|session| session.terrain_inputs().edits)
                .map_or_else(Default::default, |terrain| {
                    terrain.edits().iter().map(|stamp| stamp.id).collect()
                });
        }
        self.application.game.gameplay.collision_index = scene.collision;
        self.application.game.gameplay.collision_revision = scene.collision_revision;
        self.last_render_position = scene.prepared.as_ref().map_or(
            self.application.game.gameplay.prediction.character.position,
            |p| p.position(),
        );
        self.application.game.gameplay.sectors_ready = true;
        true
    }

    pub(in crate::renderer) fn advance_pending_transitions(&mut self) {
        let pending = self.application.pending_transition_preparations();
        for (transition_id, pending) in pending {
            let ticket =
                minimium_client_runtime::TransferTicket::new(transition_id, pending.context);
            if self.application.connection.host_readiness_requested(ticket) {
                continue;
            }
            let instance = pending.context.instance();
            let Some(terrain_inputs) = self
                .application
                .connection
                .session()
                .and_then(|session| session.terrain_inputs_for_context(pending.context))
            else {
                continue;
            };
            let water = self.water_presentation_for(pending.context);
            let scene = match self.sector_cache.ensure_scene(
                super::super::sector_scene::SceneInterestOwner::Pending(
                    transition_id,
                    pending.context,
                ),
                instance,
                pending.manifest,
                pending.destination,
                minimium_renderer::terrain_view::TerrainView::new(
                    pending.destination,
                    self.fov_degrees.to_radians(),
                    self.gpu.size.height,
                ),
                self.gpu.render_origin,
                self.gpu.world_geometry.origin_generation(),
                &self.application.world_catalog,
                &self.application.game.world_manifests,
                Some((pending.map.as_ref(), pending.manifest)),
                terrain_inputs,
                water,
            ) {
                Ok(Some(scene)) => scene,
                Ok(None) => continue,
                Err(error) => {
                    self.application
                        .connection
                        .record_diagnostic_failure(error.code(), &error.to_string());
                    if let Some(session) = self.application.connection.session_mut() {
                        let _ = session.cancel_pending_transition();
                    }
                    if let Some(failed) = self.application.fail_transition(transition_id) {
                        self.release_prepared_target(transition_id, failed.context);
                    }
                    continue;
                }
            };
            if !self.gpu.cache_sector_scene(instance, &scene) {
                continue;
            }
            let _ = self
                .application
                .connection
                .report_transfer_host_ready(ticket);
        }
    }

    pub(in crate::renderer) fn clear_pending_sector_transitions(&mut self) {
        for pending in self.application.clear_transitions() {
            self.release_prepared_target(pending.transition_id, pending.context);
        }
    }

    pub(super) fn release_prepared_target(
        &mut self,
        transition_id: minimium_world::world::TransitionId,
        context: minimium_protocol::WorldContext,
    ) {
        self.sector_cache.release_interest(
            super::super::sector_scene::SceneInterestOwner::Pending(transition_id, context),
        );
        self.gpu.release_scene_stage(
            context.instance(),
            super::super::sector_scene::SceneInterestOwner::Pending(transition_id, context),
        );
        if context.instance() != self.application.game.current_instance {
            self.release_world_sectors(context.instance());
        }
    }
}
