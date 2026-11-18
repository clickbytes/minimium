use super::{worker::CloudWorker, Publication, Request, Stamp};
use crate::renderer::{
    environment::{EnvironmentFrame, EnvironmentViewKind},
    DesktopHost,
};
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
    cover: Option<Arc<minimium_renderer::surface_cover::CoverGrid>>,
    published: Option<Arc<Publication>>,
    last_request: Option<Instant>,
    sequence: u64,
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
        if let Some((cover_stamp, cover)) = self.weather.worker.take_cover() {
            if cover_stamp == stamp
                && cover.address == minimium_renderer::surface_cover::CoverAddress::at(observer)
                && cover.start_utc == (utc / 3600.0).floor() * 3600.0
                && self
                    .weather
                    .cover
                    .as_ref()
                    .is_none_or(|active| !Arc::ptr_eq(active, &cover))
            {
                self.renderer
                    .terrain_gpu
                    .upload_cover(&self.renderer.queue, &cover);
                self.renderer
                    .atmosphere_gpu
                    .set_weather(&self.renderer.queue, &cover);
                self.weather.cover = Some(cover);
            }
        }
        self.renderer.terrain_gpu.set_cover_time(utc);
        let candidate = self.weather.worker.take().filter(|result| {
            result.request.stamp == stamp && result.request.detail == self.weather.detail
        });
        let now = Instant::now();
        if self
            .weather
            .last_request
            .is_none_or(|t| now.duration_since(t) >= Duration::from_millis(500))
        {
            self.weather.sequence += 1;
            self.weather.worker.submit(Request {
                climate,
                stamp,
                utc,
                observer,
                eye: DVec3::from_array(frame.render_view.camera_eye_world_m),
                sort: !self.renderer.cloud_gpu.oit,
                revision: self.weather.sequence,
                detail: self.weather.detail,
                started: now,
                terrain_inputs,
            });
            self.weather.last_request = Some(now);
        }
        let Some(publication) = candidate.or_else(|| self.weather.published.clone()) else {
            self.renderer.cloud_gpu.debug = false;
            return;
        };
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
        if let Err(error) = gpu.upload_frame(
            &self.renderer.queue,
            &publication.faces,
            publication.snapshot.revision,
            &records,
            &uniform,
        ) {
            self.application
                .connection_view()
                .record_diagnostic_failure("renderer.cloud_upload_rejected", &error.to_string());
            return;
        }
        gpu.standard_lighting = standard_lighting;
        gpu.debug = self.weather.debug != CloudDebug::Off;
        self.weather.published = Some(publication);
    }

    /// Read-only aiming hints for manual finite-source cloud occlusion checks.
    pub(in crate::renderer) fn cloud_targets(&self) -> Vec<String> {
        let Some(p) = &self.weather.published else {
            return vec!["dev: no published clouds".into()];
        };
        if self.render_environment_frame.context != Some(p.request.stamp.context)
            || self.application.core_view().manifest != Some(p.request.stamp.manifest)
        {
            return vec!["dev: no published clouds for the current world".into()];
        }
        let view = self.render_environment_frame.render_view;
        let eye = DVec3::from_array(view.camera_eye_world_m);
        let character = self.application.core_view().character;
        let forward = super::super::camera::camera_direction(character, 0.0, 0.0).as_dvec3();
        let right =
            super::super::camera::camera_direction(character, std::f32::consts::FRAC_PI_2, 0.0)
                .as_dvec3();
        let up =
            super::super::camera::camera_direction(character, 0.0, std::f32::consts::FRAC_PI_2)
                .as_dvec3();
        let utc = self
            .environment_frame
            .render_world_utc_ms
            .map_or(p.request.utc, |t| t as f64 / 1000.0);
        let radius = view.game_planet_radius_m
            + p.request
                .stamp
                .manifest
                .planet()
                .map_or(0.0, |planet| planet.sea_level_mm as f64 / 1000.0);
        let mut candidates: Vec<_> = p.snapshot.clouds.iter().filter_map(|cloud| {
            let age = utc - cloud.birth_utc;
            if !(420.0..=2800.0).contains(&age) { return None; }
            let center = cloud.direction_at(utc) * (radius + cloud.base_m + cloud.size_m.y * 0.5);
            let delta = center - eye;
            let distance = delta.length();
            let direction = delta.normalize_or_zero();
            let pitch = direction.dot(up).clamp(-1.0, 1.0).asin().to_degrees();
            if pitch < 5.0 { return None; }
            let yaw = direction.dot(right).atan2(direction.dot(forward)).to_degrees();
            let angular_size = (cloud.extent_m() / distance.max(1.0)).atan().to_degrees();
            Some((f64::from(cloud.opacity) * angular_size, format!(
                "cloud={} kind={:?} tau={:.2} distance_world_m={distance:.1} radius_deg={angular_size:.2} dev look {yaw:.2} {pitch:.2}",
                cloud.id, cloud.kind, cloud.opacity,
            )))
        }).collect();
        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        candidates
            .into_iter()
            .take(8)
            .map(|(_, line)| line)
            .collect()
    }

    pub(in crate::renderer) fn weather_status(&self) -> Vec<String> {
        let Some(p) = &self.weather.published else {
            return vec![
                "weather unavailable / preparing regional snapshot".into(),
                format!(
                    "terrain cover={} seasonal_phase={:.2} materials={:.1}MiB",
                    if self.renderer.terrain_gpu.uniform.cover_center[3] > 0.0 {
                        "history-grid"
                    } else {
                        "analytic-fallback"
                    },
                    self.renderer.terrain_gpu.uniform.climate[0],
                    self.renderer.terrain_gpu.bytes as f64 / 1048576.0
                ),
            ];
        };
        let s = &p.snapshot;
        let c = s.conditions;
        let utc = self
            .environment_frame
            .render_world_utc_ms
            .map_or(s.utc, |t| t as f64 / 1000.0);
        let gpu = &self.renderer.cloud_gpu;
        let wind_scale = self
            .render_environment_frame
            .render_view
            .game_planet_radius_m
            / weather::EARTH_RADIUS
            * ((1.0 - s.observer.y.powi(2)).sqrt() / 0.03).min(1.0);
        let active: Vec<_> = c
            .layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.coverage > 0.01)
            .map(|(i, l)| {
                format!(
                    "{}:{:.0}% base={:.1}m thickness={:.1}m",
                    ["low", "mid", "high"][i],
                    l.coverage * 100.0,
                    l.base_m,
                    l.thickness_m
                )
            })
            .collect();
        let kinds = weather::CloudKind::ALL
            .iter()
            .filter_map(|kind| {
                let matching: Vec<_> = s
                    .clouds
                    .iter()
                    .filter(|cloud| cloud.kind == *kind)
                    .collect();
                let base = matching
                    .iter()
                    .map(|c| c.base_m)
                    .fold(f64::INFINITY, f64::min);
                let top = matching
                    .iter()
                    .map(|c| c.base_m + c.size_m.y)
                    .fold(0.0, f64::max);
                (!matching.is_empty())
                    .then(|| format!("{}={} [{base:.1}..{top:.1}m]", kind.name(), matching.len()))
            })
            .collect::<Vec<_>>()
            .join(" ");
        let stalled = s
            .clouds
            .iter()
            .zip(&p.morphs)
            .filter(|(cloud, morph)| utc - cloud.birth_utc > morph.start + morph.step)
            .count();
        vec![
            format!(
                "terrain cover={} materials={:.1}MiB",
                if self.renderer.terrain_gpu.uniform.cover_center[3] > 0.0 {
                    "history-grid"
                } else {
                    "analytic-fallback"
                },
                self.renderer.terrain_gpu.bytes as f64 / 1048576.0
            ),
            format!(
                "weather Seasonal region={:?} T={:.1}C humidity={:.0}% water={:.0}% uplift={:.2}",
                s.region,
                c.temperature_c,
                c.humidity * 100.0,
                c.water_supply * 100.0,
                c.uplift
            ),
            format!(
                "layers [{}] wind W->E reference=5/10/20 m/s world={:.3}/{:.3}/{:.3} m/s",
                active.join(" | "),
                5.0 * wind_scale,
                10.0 * wind_scale,
                20.0 * wind_scale
            ),
            format!(
                "clouds={} faces={} regions={} rev={} limited={} mode={} GPU={:.1}MiB mesh={:.1}MiB",
                s.clouds.len(),
                p.faces.len(),
                s.regions.len(),
                s.revision,
                s.limited || p.limited,
                if gpu.oit {
                    "weighted_blended_oit"
                } else {
                    "sorted_face_alpha"
                },
                gpu.bytes() as f64 / 1048576.0,
                (p.faces.capacity() * std::mem::size_of::<super::Face>()) as f64 / 1048576.0
            ),
            format!(
                "publication age={:.2}s world_lag={:.2}s build={:.1}ms player_offset={:.4}rad debug={:?}",
                p.request.started.elapsed().as_secs_f64(),
                utc - s.utc,
                p.build_ms,
                s.observer
                    .dot(
                        DVec3::from_array(
                            self.application
                                .core_view()
                                .character
                                .position
                                .map(f64::from)
                        )
                        .normalize()
                    )
                    .clamp(-1.0, 1.0)
                    .acos(),
                self.weather.debug
            ),
            format!("families (world altitude bounds) [{kinds}]"),
            format!(
                "voxels N0..6={:?} bricks={} density_samples={} cache_hits={} cache={:.2}MiB simplified={} morph_stalled={}",
                p.stats.levels,
                p.stats.bricks,
                p.stats.samples,
                p.stats.cached,
                p.stats.cache_bytes as f64 / 1048576.0,
                p.stats.simplified,
                stalled
            ),
        ]
    }
}
