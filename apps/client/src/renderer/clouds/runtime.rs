use super::{worker::CloudWorker, Publication, Request, Stamp};
use crate::renderer::environment::{EnvironmentFrame, EnvironmentViewKind};
pub(super) use crate::renderer::DesktopHost;
use glam::{DVec3, Mat4};
use minimium_environment::weather;
use minimium_renderer::cloud_presentation::CloudDebug;
use minimium_renderer::{CloudRecord, CloudUniform};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(in crate::renderer) struct WeatherState {
    worker: CloudWorker,
    stamp: Option<Stamp>,
    climate: Option<Arc<minimium_environment::climate::ClimateScenario>>,
    weather_epoch: u64,
    pub(super) cover: Option<Arc<minimium_renderer::surface_cover::CoverGrid>>,
    pub(super) published: Option<Arc<Publication>>,
    last_request: Option<Instant>,
    sequence: u64,
    pub(super) expired: u64,
    pub(super) coarse_cover: bool,
    build_ms: f64,
    staged: Option<Arc<Publication>>,
    sorted_faces: Vec<super::Face>,
    sort_key: Option<(u64, u64, DVec3)>,
    sorted_revision: u64,
    detail: u8,
    pub(in crate::renderer) debug: CloudDebug,
}

impl WeatherState {
    pub(in crate::renderer) fn shutdown_until(&mut self, deadline: Instant) -> bool {
        self.worker.shutdown_until(deadline)
    }

    pub(in crate::renderer) fn new() -> Self {
        Self {
            worker: CloudWorker::new(),
            stamp: None,
            climate: None,
            weather_epoch: 0,
            cover: None,
            published: None,
            last_request: None,
            sequence: 0,
            expired: 0,
            coarse_cover: false,
            build_ms: 50.0,
            staged: None,
            sorted_faces: Vec::new(),
            sort_key: None,
            sorted_revision: 0,
            detail: 6,
            debug: CloudDebug::Off,
        }
    }

    pub(in crate::renderer) fn set_detail(&mut self, level: u8) {
        self.detail = level;
        self.last_request = None;
        self.worker.clear();
    }
}

impl DesktopHost {
    fn clear_weather(&mut self) {
        self.weather.cover = None;
        self.weather.staged = None;
        self.weather.stamp = None;
        self.weather.published = None;
        self.weather.last_request = None;
        self.weather.worker.clear();
        self.renderer.cloud_gpu.clear();
        self.renderer.terrain_gpu.clear_cover();
        self.renderer.atmosphere_gpu.clear_weather();
    }

    pub(in crate::renderer) fn update_weather(&mut self, frame: &EnvironmentFrame) {
        let climate = self
            .application
            .connection_view()
            .session()
            .map(|s| s.climate_scenario())
            .unwrap_or_default();
        // The inexpensive seasonal fallback is available even before cloud publication.
        if let (Some(manifest), Some(utc)) = (
            self.application.core_view().manifest,
            self.environment_frame.render_world_utc_ms,
        ) {
            let utc = utc as f64 / 1000.0;
            let seed = manifest.generation().seed().get();
            self.renderer.terrain_gpu.uniform.debug[1] = seed as u32 ^ (seed >> 64) as u32;
            self.renderer.terrain_gpu.uniform.debug[2] = (seed >> 32) as u32 ^ (seed >> 96) as u32;
            let direction =
                DVec3::from_array(frame.render_view.camera_eye_world_m).normalize_or_zero();
            let offset = climate.offset(direction, utc);
            self.renderer.terrain_gpu.uniform.climate = [
                minimium_environment::climate::seasonality(DVec3::Y, utc) as f32,
                offset.temperature_c as f32,
                offset.moisture as f32,
                climate.relief_influence as f32,
            ];
        }
        let inputs = self
            .application
            .core_view()
            .manifest
            .zip(self.environment_frame.context)
            .zip(self.environment_frame.revision)
            .zip(self.environment_frame.render_world_utc_ms);
        let Some((((manifest, context), clock_revision), utc)) =
            inputs.filter(|(((m, _), _), _)| {
                m.topology() == minimium_world::world::WorldTopology::Planet
                    && self.renderer.scene_visible
                    && frame.status
                        == crate::renderer::environment::EnvironmentFrameStatus::Available
            })
        else {
            self.clear_weather();
            return;
        };
        let Some(active_inputs) = self
            .application
            .connection_view()
            .session()
            .and_then(|session| session.terrain_inputs())
        else {
            self.clear_weather();
            return;
        };
        let Ok(terrain_inputs) = minimium_procgen::TerrainInputs::new(
            active_inputs.source().clone(),
            minimium_procgen::TerrainOverlaySnapshot::empty_for(context.instance()),
        ) else {
            return;
        };
        if self.weather.stamp.is_none() || self.weather.climate.as_ref() != Some(&climate) {
            self.weather.weather_epoch = self.weather.weather_epoch.wrapping_add(1);
            self.weather.climate = Some(climate.clone());
        }
        let stamp = Stamp {
            manifest,
            context,
            clock_revision,
            map_fingerprint: terrain_inputs.source().map_fingerprint(),
            weather_epoch: self.weather.weather_epoch,
            resolved_relief_policy: terrain_inputs.source().relief_policy(),
        };
        let utc = utc as f64 / 1000.0;
        let position = DVec3::from_array(frame.render_view.camera_eye_world_m);
        let observer = position.normalize_or_zero();
        if self.weather.stamp != Some(stamp) {
            self.weather.cover = None;
            self.renderer.terrain_gpu.clear_cover();
            self.renderer.atmosphere_gpu.clear_weather();
            self.weather.staged = None;
            self.weather.stamp = Some(stamp);
            self.weather.published = None;
            self.weather.last_request = None;
            self.weather.worker.clear();
            self.renderer.cloud_gpu.clear();
        }
        if let Some((failed_stamp, error)) = self.weather.worker.take_failure() {
            if failed_stamp == stamp {
                self.application
                    .connection_view()
                    .record_diagnostic_failure(error.code(), &error.to_string());
            }
        }
        let address = minimium_renderer::surface_cover::CoverAddress::at(observer);
        let valid_cover = |c: &minimium_renderer::surface_cover::CoverGrid| {
            c.address == address && (c.start_utc..c.start_utc + 3600.0).contains(&utc)
        };
        if self.weather.cover.as_ref().is_some_and(|c| !valid_cover(c)) {
            self.weather.cover = None;
            self.renderer.terrain_gpu.clear_cover();
            self.renderer.atmosphere_gpu.clear_weather();
        }
        if let Some((cover_stamp, cover)) = self.weather.worker.take_cover() {
            if cover_stamp == stamp && valid_cover(&cover) {
                if self
                    .weather
                    .cover
                    .as_ref()
                    .is_none_or(|old| !Arc::ptr_eq(old, &cover))
                {
                    self.install_cover(cover, false);
                }
            } else {
                self.weather.expired += 1;
            }
        }
        let now = Instant::now();
        // Reserve the adjacent revision for fallback geometry of this frame.
        self.weather.sequence += 2;
        let request = Request {
            climate,
            stamp,
            utc,
            cover_utc: utc,
            observer,
            revision: self.weather.sequence,
            detail: self.weather.detail,
            started: now,
            terrain_inputs,
        };
        if let Some(candidate) = self.weather.worker.take() {
            self.weather.build_ms = candidate.build_ms;
            if candidate.request.stamp == stamp
                && candidate.request.detail == self.weather.detail
                && candidate.near(observer)
                && candidate.interval().1 > utc
            {
                self.weather.staged = Some(candidate);
            } else {
                self.weather.expired += 1;
            }
        }
        let ready = self
            .weather
            .staged
            .as_ref()
            .is_some_and(|p| p.valid_at(utc, observer));
        let candidate = if ready {
            self.weather.staged.take()
        } else {
            None
        };
        let existing = candidate.or_else(|| {
            self.weather
                .published
                .as_ref()
                .filter(|p| p.valid_at(utc, observer))
                .cloned()
        });
        let rate = self.environment_frame.achieved_rate;
        let lead = rate * self.weather.build_ms / 1000.0;
        let active_detail = existing
            .as_ref()
            .filter(|p| !p.coarse && p.request.detail == self.weather.detail);
        // Prepare the successor while the current mesh is on screen. Waiting
        // until its last frame leaves a gap even with a fast worker.
        let running = self.environment_frame.rate_ppm.is_some_and(|ppm| ppm > 0);
        let target = if let Some(detail) = active_detail {
            if running {
                detail.interval().1.max(utc + lead)
            } else {
                utc
            }
        } else {
            utc + lead
        };
        let needs_detail = active_detail.is_none() || (running && target > utc);
        let staged_for_target = self.weather.staged.as_ref().is_some_and(|p| {
            p.request.detail == self.weather.detail && p.valid_at(target, observer)
        });
        let interval = Duration::from_secs_f64((0.5 / rate.max(1.0)).clamp(0.016, 0.5));
        if ((needs_detail && !staged_for_target)
            || self.weather.cover.is_none()
            || self.weather.coarse_cover)
            && self
                .weather
                .last_request
                .is_none_or(|t| now.duration_since(t) >= interval)
        {
            let mut next = request.clone();
            next.utc = target
                .min((minimium_environment::time::WorldTimeMs::MAX_EXCLUSIVE - 1) as f64 / 1000.0);
            if self.weather.cover.is_some() && !self.weather.coarse_cover {
                next.cover_utc = next.utc;
            }
            self.weather.worker.submit(next);
            self.weather.last_request = Some(now);
        }
        let publication = if existing.is_none() || self.weather.cover.is_none() {
            match super::selection::fallback(
                request,
                self.weather.published.as_deref(),
                self.weather.cover.is_none(),
            ) {
                Ok((local, cover)) => {
                    if let Some(cover) = cover {
                        self.install_cover(cover, true);
                    }
                    existing.unwrap_or(local)
                }
                Err(error) => {
                    self.application
                        .connection_view()
                        .record_diagnostic_failure(error.code(), &error.to_string());
                    if let Some(valid) = existing {
                        valid
                    } else {
                        self.renderer.cloud_gpu.clear();
                        self.weather.published = None;
                        return;
                    }
                }
            }
        } else {
            existing.unwrap()
        };
        self.renderer.terrain_gpu.set_cover_time(utc);
        if publication.snapshot.clouds.len() != publication.morphs.len() {
            self.application.record_diagnostic_failure(
                "renderer.cloud_publication_invalid",
                &"Cloud publication has mismatched cloud and morph counts",
            );
            return;
        }
        let gpu = &mut self.renderer.cloud_gpu;
        let view = frame.render_view;
        let origin = DVec3::from_array(view.render_origin_m);
        let radius = view.game_planet_radius_m
            + manifest
                .planet()
                .map_or(0.0, |p| p.sea_level_mm as f64 / 1000.0);
        let records: Vec<_> = publication
            .snapshot
            .clouds
            .iter()
            .zip(&publication.morphs)
            .map(|(cloud, morph)| {
                // Absolute UTC in f64; pausing, skipped frames and view changes do not integrate error.
                let up = cloud.direction_at(utc);
                let east = DVec3::new(-up.z, 0.0, up.x).normalize_or_zero();
                let center = up * (radius + cloud.base_m) - origin;
                CloudRecord {
                    half_size: (cloud.size_m * 0.5).as_vec3().extend(0.0).to_array(),
                    center_age: center
                        .as_vec3()
                        .extend((utc - cloud.birth_utc) as f32)
                        .to_array(),
                    east_opacity: east.as_vec3().extend(cloud.opacity).to_array(),
                    up_layer: up.as_vec3().extend(cloud.layer as f32).to_array(),
                    north_region: up
                        .cross(east)
                        .as_vec3()
                        .extend(weather::region::address(cloud.source) as f32)
                        .to_array(),
                    morph: [
                        morph.start as f32,
                        morph.step as f32,
                        cloud.kind as u8 as f32,
                        f32::from(morph.level),
                    ],
                }
            })
            .collect();
        let mut projection = Mat4::from_cols_array_2d(&view.projection);
        if view.kind == EnvironmentViewKind::Surface {
            let near = 0.05;
            let far = (radius as f32 * 0.2).max(1000.0);
            projection.z_axis.z = far / (near - far);
            projection.w_axis.z = near * far / (near - far);
        }
        let projection = projection * Mat4::from_cols_array_2d(&view.view);
        let target_size = gpu.targets.as_ref().map_or(view.viewport, |t| t.size);
        let mut uniform = CloudUniform {
            inside: [[0; 4]; 4],
            projection: projection.to_cols_array_2d(),
            scene_projection: view.view_projection,
            inverse_projection: projection.inverse().to_cols_array_2d(),
            origin_radius: origin.as_vec3().extend(radius as f32).to_array(),
            eye: (DVec3::from_array(view.camera_eye_world_m) - origin)
                .as_vec3()
                .extend(weather::CORE_ANGLE as f32)
                .to_array(),
            observer: observer
                .as_vec3()
                .extend(weather::OUTER_ANGLE as f32)
                .to_array(),
            config: [self.weather.debug as u32, target_size[0], target_size[1], 0],
        };
        let standard_lighting =
            self.sky_quality.effective == minimium_renderer::quality::SkyQuality::Standard;
        uniform.select_inside(&records, standard_lighting);
        let (faces, revision) = if gpu.oit {
            (&publication.faces, publication.snapshot.revision)
        } else {
            let key = (publication.snapshot.revision, utc.to_bits(), position);
            if self.weather.sort_key != Some(key) {
                self.weather.sorted_faces.clone_from(&publication.faces);
                super::selection::sort(
                    &mut self.weather.sorted_faces,
                    &publication,
                    radius,
                    utc,
                    position,
                );
                self.weather.sort_key = Some(key);
                self.weather.sorted_revision += 1;
            }
            (&self.weather.sorted_faces, self.weather.sorted_revision)
        };
        if let Err(error) =
            gpu.upload_frame(&self.renderer.queue, faces, revision, &records, &uniform)
        {
            self.application
                .connection_view()
                .record_diagnostic_failure("renderer.cloud_upload_rejected", &error.to_string());
            gpu.clear();
            self.weather.published = None;
            return;
        }
        gpu.standard_lighting = standard_lighting;
        gpu.debug = self.weather.debug != CloudDebug::Off;
        self.weather.published = Some(publication);
    }

    fn install_cover(
        &mut self,
        cover: Arc<minimium_renderer::surface_cover::CoverGrid>,
        coarse: bool,
    ) {
        self.renderer
            .terrain_gpu
            .upload_cover(&self.renderer.queue, &cover);
        self.renderer
            .atmosphere_gpu
            .set_weather(&self.renderer.queue, &cover);
        self.weather.cover = Some(cover);
        self.weather.coarse_cover = coarse;
    }
}
