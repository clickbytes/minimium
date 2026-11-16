use super::*;

pub(in crate::renderer) fn prepare_prediction_scene(
    cpu: &mut minimium_client_runtime::world_readiness::WorldReadiness,
    request: crate::application::PredictionSceneRequest<'_>,
) -> Result<Option<minimium_client_runtime::world_readiness::CollisionSnapshot>, String> {
    let owner = request.context.map_or_else(
        || super::super::sector_scene::SceneInterestOwner::Bootstrap(request.instance),
        super::super::sector_scene::SceneInterestOwner::Active,
    );
    let Some(map) = request.world_catalog.get(&request.instance) else {
        return Ok(None);
    };
    match cpu.ensure_collision_for_step(
        owner,
        request.manifest,
        request.previous_position,
        request.position,
        map,
        request.terrain_inputs.clone(),
    ) {
        Ok(snapshot) => Ok(snapshot),
        Err(error) => Err(format!("{}: {error}", error.code())),
    }
}

pub(in crate::renderer) fn prepare_prediction_presentation(
    presentation: &mut super::super::sector_scene::SectorPresentationCache,
    renderer: &mut super::Renderer,
    snapshot: &minimium_client_runtime::world_readiness::CollisionSnapshot,
    request: crate::application::PredictionPresentationRequest,
    water: Option<super::super::sector_worker::WaterPresentation>,
    fov_degrees: f32,
) -> Result<bool, String> {
    let view = minimium_renderer::terrain_view::TerrainView::new(
        request.position,
        fov_degrees.to_radians(),
        renderer.size.height,
    );
    let origin = renderer.render_origin;
    let origin_generation = renderer.world_geometry.origin_generation();
    let scene = match presentation.prepare_scene(
        snapshot.clone(),
        view,
        origin,
        origin_generation,
        request.terrain_inputs,
        water,
    ) {
        Ok(Some(scene)) => Some(scene),
        Ok(None) => return Ok(false),
        Err(error) => return Err(format!("{}: {error}", error.code())),
    };
    if let Some(scene) =
        scene.filter(|scene| request.manifest.planet().is_none() || scene.prepared.is_some())
    {
        if renderer.cache_sector_scene(snapshot.instance(), &scene)
            && renderer.activate_sector_scene(
                snapshot.instance(),
                &scene,
                snapshot.map_fingerprint(),
            )
        {
            return Ok(true);
        }
    }
    Ok(false)
}

impl DesktopHost {
    fn invalidate_active_scene(&mut self) {
        let view = self.application.core_view();
        let context = view.context;
        let instance = view.world_instance;
        self.application
            .invalidate_active_scene(context, instance, crate::host_time::sample());
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
        let current = self.application.core_view();
        let instance = current.world_instance;
        let Some(manifest) = current.manifest else {
            self.invalidate_active_scene();
            return false;
        };
        if manifest.planet().is_some() {
            let feet = current
                .position_mm
                .map_or(current.character.position.map(f64::from), |position| {
                    position.0.map(|v| v as f64 / 1000.0)
                });
            let up = minimium_world::world::character_frame(
                current.character.gravity,
                current.character.position,
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
            Ok(Some(inputs)) => inputs,
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
        let context = current.context;
        let terrain_revision = terrain_inputs.revision();
        let water = self
            .application
            .core_view()
            .context
            .and_then(|context| self.water_presentation_for(context));
        let scene = match self.sector_cache.ensure_scene(
            context.map_or_else(
                || super::super::sector_scene::SceneInterestOwner::Bootstrap(instance),
                super::super::sector_scene::SceneInterestOwner::Active,
            ),
            instance,
            manifest,
            self.application.core_view().character.position,
            view,
            self.renderer.render_origin,
            self.renderer.world_geometry.origin_generation(),
            &self.application.world_catalog_view(),
            &self.application.world_manifest_catalog_view(),
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
        if !self.renderer.cache_sector_scene(instance, &scene)
            || !self.renderer.activate_sector_scene(
                instance,
                &scene,
                self.application
                    .world_catalog_view()
                    .get(&instance)
                    .map_or(0, |map| map.fingerprint()),
            )
        {
            let current = context.map(|context| (context, terrain_revision));
            if self.impacts.published_terrain != current {
                self.invalidate_active_scene();
                return false;
            }
            if !self
                .application
                .report_active_scene_ready(scene.cpu.clone(), crate::host_time::sample())
            {
                return false;
            }
            self.last_render_position = [f32::INFINITY; 3];
            return true;
        }
        if let (Some(context), Some(prepared)) = (current.context, scene.prepared.as_ref()) {
            self.impacts.published_terrain = Some((context, prepared.terrain_revision()));
            self.impacts.published_craters = self
                .application
                .connection_view()
                .session()
                .and_then(|session| session.terrain_inputs())
                .and_then(|inputs| inputs.overlays().edits().cloned())
                .map_or_else(Default::default, |terrain| {
                    terrain.edits().iter().map(|stamp| stamp.id).collect()
                });
        }
        if !self
            .application
            .report_active_scene_ready(scene.cpu.clone(), crate::host_time::sample())
        {
            return false;
        }
        self.last_render_position = scene
            .prepared
            .as_ref()
            .map_or(self.application.core_view().character.position, |p| {
                p.position()
            });
        true
    }

    pub(in crate::renderer) fn advance_pending_transitions(&mut self) {
        let view = self.application.desktop_render_view();
        let Some(pending) = view.pending.as_ref() else {
            return;
        };
        let ticket = pending.ticket;
        let transition = pending.transition;
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
            self.application.world_manifest_catalog_view(),
            Some((transition.map.as_ref(), transition.manifest)),
            terrain_inputs,
            water,
        ) {
            Ok(Some(scene)) => scene,
            Ok(None) => return,
            Err(error) => {
                self.application
                    .record_diagnostic_failure(error.code(), &error.to_string());
                if let Err(error) = self
                    .application
                    .request_pending_transition_cancel(crate::host_time::sample())
                {
                    self.application
                        .record_diagnostic_failure("client.invalid_time", &error);
                }
                if let Some(failed) = self.application.fail_transition(transition_id) {
                    self.release_prepared_target(transition_id, failed.context);
                }
                return;
            }
        };
        if !self.renderer.cache_sector_scene(instance, &scene) {
            return;
        }
        let ready = crate::application::TransferPresentationReady::new(ticket, scene.cpu.clone());
        let _ = self
            .application
            .report_transfer_presentation_ready(ready, crate::host_time::sample());
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
        if context.instance() != self.application.core_view().world_instance {
            self.release_world_sectors(context.instance());
        }
    }
}
