use super::{
    gpu::{CloudRecord, CloudUniform},
    worker::CloudWorker,
    Publication, Request, Stamp,
};
use crate::{
    environment::weather::{self, CloudDebug},
    renderer::{
        environment::{EnvironmentFrame, EnvironmentViewKind},
        Renderer,
    },
};
use glam::{DVec3, Mat4};
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
        let inputs = self
            .game
            .world_manifests
            .get(&self.game.current_instance)
            .copied()
            .zip(self.environment_frame.context)
            .zip(self.environment_frame.revision)
            .zip(self.environment_frame.render_world_utc_ms);
        let Some((((manifest, context), clock_revision), utc)) =
            inputs.filter(|(((m, _), _), _)| {
                m.topology() == minimium_core::world::WorldTopology::Planet
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
            self.gpu.cloud_gpu.debug = false;
            return;
        };
        let stamp = Stamp {
            manifest,
            context,
            clock_revision,
        };
        let utc = utc as f64 / 1000.0;
        let position = self.game.position_mm.map_or_else(
            || DVec3::from_array(self.game.character.position.map(f64::from)),
            |p| DVec3::from_array(p.0.map(|v| v as f64 / 1000.0)),
        );
        let observer = position.normalize_or_zero();
        if self.weather.stamp != Some(stamp) {
            self.weather.stamp = Some(stamp);
            self.weather.published = None;
            self.weather.last_request = None;
            self.weather.worker.clear();
            self.gpu.cloud_gpu.count = 0;
        }
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
                stamp,
                utc,
                observer,
                eye: DVec3::from_array(frame.render_view.camera_eye_world_m),
                sort: !self.gpu.cloud_gpu.oit,
                revision: self.weather.sequence,
                detail: self.weather.detail,
                started: now,
            });
            self.weather.last_request = Some(now);
        }
        let Some(publication) = &self.weather.published else {
            self.gpu.cloud_gpu.debug = false;
            return;
        };
        let gpu = &mut self.gpu.cloud_gpu;
        if gpu.revision != publication.snapshot.revision {
            if !publication.faces.is_empty() {
                self.gpu.queue.write_buffer(
                    &gpu.faces,
                    0,
                    bytemuck::cast_slice(&publication.faces),
                );
            }
            gpu.count = publication.faces.len() as u32;
            gpu.revision = publication.snapshot.revision;
        }
        let view = frame.render_view;
        let origin = DVec3::from_array(view.render_origin_m);
        let radius = view.game_planet_radius_m;
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
        if !records.is_empty() {
            self.gpu
                .queue
                .write_buffer(&gpu.records, 0, bytemuck::cast_slice(&records));
        }
        let mut projection = Mat4::from_cols_array_2d(&view.projection);
        if view.kind == EnvironmentViewKind::Surface {
            let near = 0.05;
            let far = (radius as f32 * 0.2).max(1000.0);
            projection.z_axis.z = far / (near - far);
            projection.w_axis.z = near * far / (near - far);
        }
        let projection = projection * Mat4::from_cols_array_2d(&view.view);
        let target_size = gpu.targets.as_ref().map_or(view.viewport, |t| t.size);
        let uniform = CloudUniform {
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
        self.gpu
            .queue
            .write_buffer(&gpu.uniform, 0, bytemuck::bytes_of(&uniform));
        gpu.debug = self.weather.debug != CloudDebug::Off;
    }

    pub(in crate::renderer) fn weather_status(&self) -> Vec<String> {
        let Some(p) = &self.weather.published else {
            return vec!["weather unavailable / preparing regional snapshot".into()];
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
            format!("weather AnnualMean region={:?} T={:.1}C humidity={:.0}% water={:.0}% uplift={:.2}", s.region, c.temperature_c, c.humidity * 100.0, c.water_supply * 100.0, c.uplift),
            format!("layers [{}] wind W->E reference=5/10/20 m/s world={:.3}/{:.3}/{:.3} m/s", active.join(" | "), 5.0 * wind_scale, 10.0 * wind_scale, 20.0 * wind_scale),
            format!("clouds={} faces={} regions={} rev={} limited={} mode={} GPU={:.1}MiB mesh={:.1}MiB", s.clouds.len(), p.faces.len(), s.regions.len(), s.revision, s.limited || p.limited, if gpu.oit { "weighted_blended_oit" } else { "sorted_face_alpha" }, gpu.bytes() as f64 / 1048576.0, (p.faces.capacity() * std::mem::size_of::<super::Face>()) as f64 / 1048576.0),
            format!("publication age={:.2}s world_lag={:.2}s build={:.1}ms player_offset={:.4}rad debug={:?}", p.request.started.elapsed().as_secs_f64(), utc - s.utc, p.build_ms, s.observer.dot(DVec3::from_array(self.game.character.position.map(f64::from)).normalize()).clamp(-1.0, 1.0).acos(), self.weather.debug),
            format!("families (world altitude bounds) [{kinds}]"),
            format!("voxels N0..6={:?} bricks={} density_samples={} cache_hits={} cache={:.2}MiB simplified={} morph_stalled={}", p.stats.levels, p.stats.bricks, p.stats.samples, p.stats.cached, p.stats.cache_bytes as f64 / 1048576.0, p.stats.simplified, stalled),
        ]
    }
}
