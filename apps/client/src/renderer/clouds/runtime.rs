use super::{worker::CloudWorker, Publication, Request, Stamp};
use crate::renderer::{
    environment::{EnvironmentFrame, EnvironmentViewKind},
    Renderer,
};
use glam::{DVec3, Mat4};
use minimium_environment::weather::{self, CloudDebug};
use minimium_renderer::{CloudRecord, CloudUniform};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

pub(in crate::renderer) struct WeatherState {
    worker: CloudWorker,
    stamp: Option<Stamp>,
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

impl Renderer {
    pub(in crate::renderer) fn update_weather(&mut self, frame: &EnvironmentFrame) {
        // The inexpensive seasonal fallback is available even before cloud publication.
        if let (Some(manifest), Some(utc)) = (
            self.application
                .game
                .world_manifests
                .get(&self.application.game.current_instance),
            self.environment_frame.render_world_utc_ms,
        ) {
            let utc = utc as f64 / 1000.0;
            let seed = manifest.generation().seed().get();
            self.gpu.terrain_gpu.uniform.debug[1] = seed as u32 ^ (seed >> 64) as u32;
            self.gpu.terrain_gpu.uniform.debug[2] = (seed >> 32) as u32 ^ (seed >> 96) as u32;
            let direction =
                DVec3::from_array(frame.render_view.camera_eye_world_m).normalize_or_zero();
            let scenario = self
                .application
                .connection
                .session()
                .map(|s| s.climate_scenario())
                .unwrap_or_default();
            let offset = scenario.offset(direction, utc);
            self.gpu.terrain_gpu.uniform.climate = [
                minimium_environment::climate::seasonality(DVec3::Y, utc) as f32,
                offset.temperature_c as f32,
                offset.moisture as f32,
                scenario.relief_influence as f32,
            ];
        }
        let inputs = self
            .application
            .game
            .world_manifests
            .get(&self.application.game.current_instance)
            .copied()
            .zip(self.environment_frame.context)
            .zip(self.environment_frame.revision)
            .zip(self.environment_frame.render_world_utc_ms);
        let Some((((manifest, context), clock_revision), utc)) =
            inputs.filter(|(((m, _), _), _)| {
                m.topology() == minimium_world::world::WorldTopology::Planet
                    && self.gpu.scene_visible
                    && frame.status
                        == crate::renderer::environment::EnvironmentFrameStatus::Available
            })
        else {
            self.weather.stamp = None;
            self.weather.published = None;
            self.weather.last_request = None;
            self.weather.worker.clear();
            self.gpu.cloud_gpu.count = 0;
            self.gpu.cloud_gpu.inside_count = 0;
            self.gpu.cloud_gpu.light_records.clear();
            self.gpu.cloud_gpu.debug = false;
            self.gpu.terrain_gpu.clear_cover();
            self.gpu.atmosphere_gpu.clear_weather();
            return;
        };
        let stamp = Stamp {
            manifest,
            context,
            clock_revision,
        };
        let utc = utc as f64 / 1000.0;
        let position = DVec3::from_array(frame.render_view.camera_eye_world_m);
        let observer = position.normalize_or_zero();
        if self.weather.stamp != Some(stamp) {
            self.gpu.terrain_gpu.clear_cover();
            self.gpu.atmosphere_gpu.clear_weather();
            self.weather.stamp = Some(stamp);
            self.weather.published = None;
            self.weather.last_request = None;
            self.weather.worker.clear();
            self.gpu.cloud_gpu.count = 0;
            self.gpu.cloud_gpu.inside_count = 0;
            self.gpu.cloud_gpu.light_records.clear();
        }
        if let Some((cover_stamp, cover)) = self.weather.worker.take_cover() {
            if cover_stamp == stamp {
                self.gpu.terrain_gpu.upload_cover(&self.gpu.queue, &cover);
                self.gpu.atmosphere_gpu.set_weather(&self.gpu.queue, &cover);
            }
        }
        self.gpu.terrain_gpu.set_cover_time(utc);
        if let Some(result) = self.weather.worker.take() {
            if result.request.stamp == stamp && result.request.detail == self.weather.detail {
                self.weather.published = Some(result);
            }
        }
        let now = Instant::now();
        if self
            .weather
            .last_request
            .is_none_or(|t| now.duration_since(t) >= Duration::from_millis(500))
        {
            self.weather.sequence += 1;
            self.weather.worker.submit(Request {
                climate: self
                    .application
                    .connection
                    .session()
                    .map(|s| s.climate_scenario())
                    .unwrap_or_default(),
                stamp,
                utc,
                observer,
                eye: DVec3::from_array(frame.render_view.camera_eye_world_m),
                sort: !self.gpu.cloud_gpu.oit,
                revision: self.weather.sequence,
                detail: self.weather.detail,
                started: now,
                geography: self
                    .application
                    .connection
                    .session()
                    .and_then(|session| session.prepared_geography()),
            });
            self.weather.last_request = Some(now);
        }
        let Some(publication) = &self.weather.published else {
            self.gpu.cloud_gpu.debug = false;
            return;
        };
        let gpu = &mut self.gpu.cloud_gpu;
        gpu.update_faces(
            &self.gpu.queue,
            &publication.faces,
            publication.snapshot.revision,
        );
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
        gpu.standard_lighting =
            self.sky_quality.effective == minimium_renderer::quality::SkyQuality::Standard;
        uniform.select_inside(
            &records,
            self.sky_quality.effective == minimium_renderer::quality::SkyQuality::Standard,
        );
        gpu.upload_frame(&self.gpu.queue, &records, &uniform);
        gpu.debug = self.weather.debug != CloudDebug::Off;
    }

    /// Read-only aiming hints for manual finite-source cloud occlusion checks.
    pub(in crate::renderer) fn cloud_targets(&self) -> Vec<String> {
        let Some(p) = &self.weather.published else {
            return vec!["dev: no published clouds".into()];
        };
        let view = self.render_environment_frame.render_view;
        let eye = DVec3::from_array(view.camera_eye_world_m);
        let character = self.application.game.gameplay.prediction.character;
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
                    if self.gpu.terrain_gpu.uniform.cover_center[3] > 0.0 {
                        "history-grid"
                    } else {
                        "analytic-fallback"
                    },
                    self.gpu.terrain_gpu.uniform.climate[0],
                    self.gpu.terrain_gpu.bytes as f64 / 1048576.0
                ),
            ];
        };
        let s = &p.snapshot;
        let c = s.conditions;
        let utc = self
            .environment_frame
            .render_world_utc_ms
            .map_or(s.utc, |t| t as f64 / 1000.0);
        let gpu = &self.gpu.cloud_gpu;
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
                if self.gpu.terrain_gpu.uniform.cover_center[3] > 0.0 {
                    "history-grid"
                } else {
                    "analytic-fallback"
                },
                self.gpu.terrain_gpu.bytes as f64 / 1048576.0
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
                                .game
                                .gameplay
                                .prediction
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
