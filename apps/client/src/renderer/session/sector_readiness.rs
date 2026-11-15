use super::*;

pub(in crate::renderer) fn prepare_prediction_scene(
    sector_cache: &mut super::super::sector_scene::ClientSectorCache,
    renderer: &mut super::Renderer,
    request: crate::application::PredictionSceneRequest<'_>,
    water: Option<super::super::sector_worker::WaterPresentation>,
    fov_degrees: f32,
    published_terrain: Option<(
        minimium_protocol::WorldContext,
        minimium_procgen::SectorContentVersion,
    )>,
) -> Result<Option<crate::application::ActiveSceneReady>, String> {
    if !request.session_content_ready {
        return Ok(None);
    }
    let owner = request.context.map_or_else(
        || super::super::sector_scene::SceneInterestOwner::Bootstrap(request.instance),
        super::super::sector_scene::SceneInterestOwner::Active,
    );
    let view = minimium_renderer::terrain_view::TerrainView::new(
        request.position,
        fov_degrees.to_radians(),
        renderer.size.height,
    );
    let origin = renderer.render_origin;
    let origin_generation = renderer.world_geometry.origin_generation();
    let terrain_revision = request.terrain_inputs.revision();
    let scene = match sector_cache.ensure_scene(
        owner,
        request.instance,
        request.manifest,
        request.position,
        view,
        origin,
        origin_generation,
        request.world_catalog,
        request.world_manifests,
        None,
        request.terrain_inputs,
        water,
    ) {
        Ok(Some(scene)) => scene,
        Ok(None) => return Ok(None),
        Err(error) => return Err(format!("{}: {error}", error.code())),
    };
    if !request.water_covers_candidate {
        return Ok(None);
    }
    if request.manifest.planet().is_some() && scene.prepared.is_none() {
        return Ok(None);
    }
    let map_fingerprint = request
        .world_catalog
        .get(&request.instance)
        .map_or(0, |map| map.fingerprint());
    if !renderer.cache_sector_scene(request.instance, &scene)
        || !renderer.activate_sector_scene(request.instance, &scene, map_fingerprint)
    {
        let current = request.context.map(|context| (context, terrain_revision));
        if current.is_none() || published_terrain != current {
            return Ok(None);
        }
    }
    Ok(Some(
        crate::application::ActiveSceneReady::new_for_candidate(
            request.context,
            request.instance,
            request.manifest,
            map_fingerprint,
            terrain_revision,
            scene.collision_revision,
            scene.collision,
            request.position,
        ),
    ))
}

impl DesktopHost {
    fn invalidate_active_scene(&mut self) {
        let view = self.application.game_view();
        let context = view.session_context();
        let instance = view.current_instance;
        self.application.invalidate_active_scene(context, instance);
    }

    pub(in crate::renderer) fn water_presentation_for(
        &self,
        context: minimium_protocol::WorldContext,
    ) -> Option<super::super::sector_worker::WaterPresentation> {
        let session = self.application.connection_view().session()?;
        let catalog = session.water_catalog_for_context(context)?;
        let snapshot = session.fluid_snapshot()?;
        (snapshot.world() == context.instance())
            .then(|| super::super::sector_worker::WaterPresentation::new(catalog, snapshot))
    }

    pub(in crate::renderer) fn refresh_active_sector_scene(&mut self) -> bool {
        if self
            .application
            .connection_view()
            .session()
            .is_some_and(|s| !s.impact_ready() || !s.excavation_ready())
        {
            self.invalidate_active_scene();
            return false;
        }
        let Some(manifest) = self
            .application
            .game_view()
            .world_manifests
            .get(&self.application.game_view().current_instance)
            .copied()
        else {
            self.invalidate_active_scene();
            return false;
        };
        if manifest.planet().is_some() {
            let prediction = &self.application.game_view().gameplay.prediction;
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
                .connection_view()
                .session()
                .is_some_and(|session| session.water_region_covers_capsule(feet, up, 1.7))
            {
                self.invalidate_active_scene();
                return false;
            }
        }
        let view = self.terrain_view();
        let terrain_inputs = match self.application.scene_terrain_inputs() {
            Ok(inputs) => inputs,
            Err(error) => {
                self.application
                    .record_diagnostic_failure(error.code(), &error.to_string());
                self.invalidate_active_scene();
                return false;
            }
        };
        let context = self.application.game_view().session_context();
        let terrain_revision = terrain_inputs.revision();
        let water = self
            .application
            .game_view()
            .session_context()
            .and_then(|context| self.water_presentation_for(context));
        let scene = match self.sector_cache.ensure_scene(
            self.application.game_view().session_context().map_or_else(
                || {
                    super::super::sector_scene::SceneInterestOwner::Bootstrap(
                        self.application.game_view().current_instance,
                    )
                },
                super::super::sector_scene::SceneInterestOwner::Active,
            ),
            self.application.game_view().current_instance,
            manifest,
            self.application
                .game_view()
                .gameplay
                .prediction
                .character
                .position,
            view,
            self.renderer.render_origin,
            self.renderer.world_geometry.origin_generation(),
            &self.application.world_catalog_view(),
            &self.application.game_view().world_manifests,
            None,
            terrain_inputs,
            water,
        ) {
            Ok(Some(scene)) => scene,
            Ok(None) => {
                self.invalidate_active_scene();
                return false;
            }
            Err(error) => {
                self.application
                    .record_diagnostic_failure(error.code(), &error.to_string());
                self.invalidate_active_scene();
                return false;
            }
        };
        if manifest.planet().is_some() && scene.prepared.is_none() {
            self.invalidate_active_scene();
            return false;
        }
        if !self
            .renderer
            .cache_sector_scene(self.application.game_view().current_instance, &scene)
            || !self.renderer.activate_sector_scene(
                self.application.game_view().current_instance,
                &scene,
                self.application
                    .world_catalog_view()
                    .get(&self.application.game_view().current_instance)
                    .map_or(0, |map| map.fingerprint()),
            )
        {
            let current = context.map(|context| (context, terrain_revision));
            if self.impacts.published_terrain != current {
                self.invalidate_active_scene();
                return false;
            }
            if !self.application.report_active_scene_ready(
                crate::application::ActiveSceneReady::new(
                    context,
                    self.application.game_view().current_instance,
                    manifest,
                    self.application
                        .world_catalog_view()
                        .get(&self.application.game_view().current_instance)
                        .map_or(0, |map| map.fingerprint()),
                    terrain_revision,
                    scene.collision_revision,
                    scene.collision,
                ),
            ) {
                return false;
            }
            self.last_render_position = [f32::INFINITY; 3];
            return true;
        }
        if let (Some(context), Some(prepared)) = (
            self.application.game_view().session_context(),
            scene.prepared.as_ref(),
        ) {
            self.impacts.published_terrain = Some((context, prepared.terrain_revision()));
            self.impacts.published_craters = self
                .application
                .connection_view()
                .session()
                .and_then(|session| session.terrain_inputs().edits)
                .map_or_else(Default::default, |terrain| {
                    terrain.edits().iter().map(|stamp| stamp.id).collect()
                });
        }
        if !self
            .application
            .report_active_scene_ready(crate::application::ActiveSceneReady::new(
                context,
                self.application.game_view().current_instance,
                manifest,
                self.application
                    .world_catalog_view()
                    .get(&self.application.game_view().current_instance)
                    .map_or(0, |map| map.fingerprint()),
                terrain_revision,
                scene.collision_revision,
                scene.collision,
            ))
        {
            return false;
        }
        self.last_render_position = scene.prepared.as_ref().map_or(
            self.application
                .game_view()
                .gameplay
                .prediction
                .character
                .position,
            |p| p.position(),
        );
        true
    }

    pub(in crate::renderer) fn advance_pending_transitions(&mut self) {
        let view = self.application.desktop_render_view();
        let Some(pending) = view.pending.as_ref() else {
            return;
        };
        let ticket = pending.ticket;
        let transition = pending.transition;
        let terrain_revision = pending.terrain_inputs.revision();
        let terrain_inputs = pending.terrain_inputs.clone();
        let transition_id = ticket.transition_id();
        let context = ticket.context();
        let instance = context.instance();
        let water = self.water_presentation_for(context);
        let scene = match self.sector_cache.ensure_scene(
            super::super::sector_scene::SceneInterestOwner::Pending(transition_id, context),
            instance,
            transition.manifest,
            transition.destination,
            minimium_renderer::terrain_view::TerrainView::new(
                transition.destination,
                self.fov_degrees.to_radians(),
                self.renderer.size.height,
            ),
            self.renderer.render_origin,
            self.renderer.world_geometry.origin_generation(),
            view.world_catalog,
            &self.application.game_view().world_manifests,
            Some((transition.map.as_ref(), transition.manifest)),
            terrain_inputs,
            water,
        ) {
            Ok(Some(scene)) => scene,
            Ok(None) => return,
            Err(error) => {
                self.application
                    .record_diagnostic_failure(error.code(), &error.to_string());
                self.application.request_pending_transition_cancel();
                if let Some(failed) = self.application.fail_transition(transition_id) {
                    self.release_prepared_target(transition_id, failed.context);
                }
                return;
            }
        };
        if !self.renderer.cache_sector_scene(instance, &scene) {
            return;
        }
        let ready = crate::application::TransferPresentationReady::new(ticket, terrain_revision);
        let _ = self.application.report_transfer_presentation_ready(ready);
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
        self.renderer.release_scene_stage(
            context.instance(),
            super::super::sector_scene::SceneInterestOwner::Pending(transition_id, context),
        );
        if context.instance() != self.application.game_view().current_instance {
            self.release_world_sectors(context.instance());
        }
    }
}
