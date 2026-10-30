use super::super::Renderer;
use super::{
    EnvironmentFrame, EnvironmentSettings, PreparedEnvironmentFrame, PreparedStarUpload,
    RenderViewFrame, SkyFrameInput, StarFrameStamp,
};
use minimium_astronomy::{SkyInvalidation, SkySampleContext};
use minimium_core::{
    environment::{AstronomyModelRevision, CatalogueId},
    sky::{resolve_sky_model, SkyModel, DEFAULT_OBSERVER_EYE_HEIGHT_MM},
    time::{ClockRevision, WorldTimeMs},
    world::Position3Mm,
};
use minimium_protocol::WorldContext;

impl Renderer {
    pub(in crate::renderer) fn update_render_environment(
        &mut self,
        render_view: RenderViewFrame,
    ) -> PreparedEnvironmentFrame {
        let started = std::time::Instant::now();
        let stars = self.update_render_environment_inner(render_view);
        let prepared = PreparedEnvironmentFrame::new(self.render_environment_frame.clone(), stars);
        self.metrics
            .frame_stats
            .record_environment_cpu(started.elapsed());
        let counters = self.sky_calculator.diagnostics();
        self.metrics.frame_stats.set_astronomy_counters(
            counters.ephemeris_evaluations,
            counters.ephemeris_cache_hits,
            counters.star_epoch_updates,
            counters.star_epoch_cache_hits,
        );
        prepared
    }

    fn update_render_environment_inner(
        &mut self,
        render_view: RenderViewFrame,
    ) -> Option<PreparedStarUpload> {
        if !self.gpu.scene_visible {
            self.environment_observer.clear();
            self.render_environment_frame = EnvironmentFrame::unavailable(render_view);
            self.environment_error = None;
            return None;
        }
        let input = self.environment_frame;
        let Some(descriptor) = input.descriptor else {
            self.environment_observer.clear();
            self.render_environment_frame = EnvironmentFrame::unavailable(render_view);
            self.environment_error = None;
            return None;
        };
        let Some(context) = input.context else {
            self.publish_unavailable(render_view, "active world context is unavailable");
            return None;
        };
        let Some(utc_ms) = input
            .render_world_utc_ms
            .filter(|_| input.estimated_world_utc_ms.is_some())
        else {
            self.publish_unavailable(render_view, "world UTC is unavailable");
            return None;
        };
        let Some(revision) = input
            .revision
            .and_then(|value| ClockRevision::new(value).ok())
        else {
            self.publish_unavailable(render_view, "clock revision is invalid");
            return None;
        };
        let Ok(utc_ms) = WorldTimeMs::new(utc_ms) else {
            self.publish_unavailable(render_view, "world UTC is outside the astronomy range");
            return None;
        };
        let Some(manifest) = self.world_manifests.get(&self.current_instance).copied() else {
            self.publish_unavailable(render_view, "world manifest is unavailable");
            return None;
        };
        if context.instance() != self.current_instance
            || context.content_revision() != manifest.content_revision()
        {
            self.publish_unavailable(
                render_view,
                "world context does not match the active manifest",
            );
            return None;
        }
        if !self.descriptor_matches_catalogue(descriptor) {
            self.publish_unavailable(render_view, "environment model or catalogue does not match");
            return None;
        }

        match resolve_sky_model(manifest, descriptor) {
            Ok(SkyModel::None) => {
                match EnvironmentFrame::interior(context, revision, descriptor, render_view) {
                    Ok(frame) => self.publish_environment(frame),
                    Err(error) => self.publish_unavailable(render_view, error.to_string()),
                }
                None
            }
            Ok(_) => self.build_planet_frame(
                context,
                descriptor,
                revision,
                utc_ms,
                manifest,
                render_view,
            ),
            Err(error) => {
                self.publish_unavailable(render_view, error.to_string());
                None
            }
        }
    }

    fn build_planet_frame(
        &mut self,
        context: WorldContext,
        descriptor: minimium_core::environment::EnvironmentDescriptor,
        revision: ClockRevision,
        utc_ms: WorldTimeMs,
        manifest: minimium_core::world::WorldManifest,
        render_view: RenderViewFrame,
    ) -> Option<PreparedStarUpload> {
        let feet_position_mm = self.game.position_mm.or_else(|| {
            (self.environment_frame.quality
                == crate::environment::SynchronizationQuality::LocalDemo)
                .then(|| demo_position_mm(self.game.character.position))
                .flatten()
        });
        let observer = match self.environment_observer.update(
            context,
            manifest,
            descriptor,
            feet_position_mm,
            DEFAULT_OBSERVER_EYE_HEIGHT_MM,
        ) {
            Ok(Some(observer)) => observer,
            Ok(None) => {
                self.publish_unavailable(render_view, "planet observer is unavailable");
                return None;
            }
            Err(error) => {
                self.publish_unavailable(render_view, error.to_string());
                return None;
            }
        };
        let sample = SkySampleContext {
            utc_ms,
            clock_revision: revision,
            world_context_key: minimium_astronomy::AstronomyContextKey::new(
                context.instance(),
                context.content_revision(),
                context.membership_epoch(),
            ),
            observer_model_revision: observer.height_reference_revision,
        };
        let sky = match self
            .sky_calculator
            .evaluate(sample, observer, SkyInvalidation::None)
        {
            Ok(sky) => sky,
            Err(error) => {
                self.publish_unavailable(render_view, error.to_string());
                return None;
            }
        };
        if sky.observer_model_revision != observer.height_reference_revision {
            self.publish_unavailable(
                render_view,
                "sky result does not match the observer profile",
            );
            return None;
        }
        let Some(catalogue) = self.sky_catalogue.as_ref() else {
            self.publish_unavailable(render_view, "embedded star catalog is unavailable");
            return None;
        };
        let corrected = match self
            .sky_calculator
            .update_star_epoch(utc_ms, catalogue.astrometry())
        {
            Ok(directions) => directions,
            Err(error) => {
                self.publish_unavailable(render_view, error.to_string());
                return None;
            }
        };
        let star_stamp = StarFrameStamp {
            context,
            clock_revision: revision,
            utc_day: utc_ms.get().div_euclid(86_400_000),
            epoch_revision: corrected.star_epoch_revision,
        };
        let star_upload = if self.gpu.stars.uploaded_stamp() != Some(star_stamp) {
            let instances = match catalogue.prepare_gpu_instances(&corrected) {
                Ok(instances) => instances,
                Err(error) => {
                    self.publish_unavailable(render_view, error.to_string());
                    return None;
                }
            };
            Some(PreparedStarUpload {
                instances,
                stamp: star_stamp,
            })
        } else {
            None
        };
        let demo_catalogue = self
            .demo_universe_enabled
            .then(super::catalogue::demo_catalogue);
        let catalog_observer =
            super::catalogue::observer_epoch_position(&sky, observer.game_eye_world_m);
        let time_seconds = utc_ms.get() as f64 / 1000.0;
        let catalog_input =
            demo_catalogue.map(|demo| (&demo.catalog, demo.system, catalog_observer, time_seconds));
        let observations = minimium_astronomy::observed_bodies(&sky, catalog_input);
        match EnvironmentFrame::from_sky_state(
            &sky,
            SkyFrameInput {
                observer,
                context,
                clock_revision: revision,
                descriptor,
                render_view,
                observed_bodies: observations,
                settings: EnvironmentSettings::planetary(),
            },
        ) {
            Ok(mut frame) => {
                frame.diagnostics.star_frame = Some(star_stamp);
                self.publish_environment(frame);
                star_upload
            }
            Err(error) => {
                self.publish_unavailable(render_view, error.to_string());
                None
            }
        }
    }

    fn descriptor_matches_catalogue(
        &self,
        descriptor: minimium_core::environment::EnvironmentDescriptor,
    ) -> bool {
        let Some(catalogue) = self.sky_catalogue.as_ref() else {
            return false;
        };
        let identity = catalogue.identity();
        descriptor.astronomy_model_revision() == AstronomyModelRevision::CURRENT
            && descriptor.catalogue_id() == CatalogueId::CURRENT
            && identity.catalogue_id == descriptor.catalogue_id().get()
            && identity.sha256 == *descriptor.catalogue_sha256().as_bytes()
    }

    fn publish_environment(&mut self, frame: EnvironmentFrame) {
        self.render_environment_frame = frame;
        self.environment_error = None;
    }

    fn publish_unavailable(&mut self, view: RenderViewFrame, reason: impl Into<String>) {
        let reason = reason.into();
        if self.environment_error.as_deref() != Some(reason.as_str()) {
            self.ui
                .console
                .push_error(format!("Environment unavailable: {reason}"));
        }
        self.environment_error = Some(reason);
        self.render_environment_frame = EnvironmentFrame::unavailable(view);
    }
}

fn demo_position_mm(position: [f32; 3]) -> Option<Position3Mm> {
    let mut millimetres = [0_i64; 3];
    for (output, coordinate) in millimetres.iter_mut().zip(position) {
        let value = f64::from(coordinate) * 1000.0;
        if !value.is_finite() || value.abs() > i64::MAX as f64 {
            return None;
        }
        *output = value.round() as i64;
    }
    Some(Position3Mm(millimetres))
}
