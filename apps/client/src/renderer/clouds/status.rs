use super::runtime::DesktopHost;
use glam::DVec3;
struct CloudTarget {
    id: u64,
    kind: String,
    opacity: f64,
    distance_m: f64,
    radius_deg: f64,
    yaw_deg: f64,
    pitch_deg: f64,
    score: f64,
}

impl DesktopHost {
    pub(in crate::renderer) fn weather_report(
        &self,
    ) -> Option<minimium_commands::operation::CommandValue> {
        use super::super::render_queries::{finite, record};
        use minimium_commands::operation::CommandValue as V;
        let p = self.weather.published.as_ref()?;
        if self.render_environment_frame.context != Some(p.request.stamp.context)
            || self.application.core_view().manifest != Some(p.request.stamp.manifest)
        {
            return None;
        }
        let s = &p.snapshot;
        Some(record([
            ("snapshot_revision", V::U64(s.revision)),
            ("clouds", V::U64(s.clouds.len() as u64)),
            ("regions", V::U64(s.regions.len() as u64)),
            ("faces", V::U64(p.faces.len() as u64)),
            ("limited", V::Bool(s.limited || p.limited)),
            ("coarse", V::Bool(p.coarse)),
            ("build_ms", finite(p.build_ms)),
            (
                "sample_age_ms",
                V::U64(
                    p.request
                        .started
                        .elapsed()
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64,
                ),
            ),
            ("cloud_gpu_bytes", V::U64(self.renderer.cloud_gpu.bytes())),
            ("expired_publications", V::U64(self.weather.expired)),
        ]))
    }

    pub(in crate::renderer) fn cloud_targets_report(
        &self,
    ) -> Option<minimium_commands::operation::CommandValue> {
        use super::super::render_queries::{finite, record};
        use minimium_commands::operation::CommandValue as V;
        let publication = self.weather.published.as_ref()?;
        let targets = self.cloud_target_candidates()?;
        let rows = targets
            .into_iter()
            .map(|target| {
                record([
                    ("id", V::U64(target.id)),
                    ("kind", V::Enum(target.kind)),
                    ("opacity", finite(target.opacity)),
                    ("distance_m", finite(target.distance_m)),
                    ("radius_deg", finite(target.radius_deg)),
                    ("yaw_deg", finite(target.yaw_deg)),
                    ("pitch_deg", finite(target.pitch_deg)),
                ])
            })
            .collect();
        Some(record([
            (
                "rows",
                V::Page {
                    rows,
                    next_cursor: None,
                },
            ),
            ("limit", V::U64(8)),
            ("snapshot_revision", V::U64(publication.snapshot.revision)),
            (
                "sample_age_ms",
                V::U64(
                    publication
                        .request
                        .started
                        .elapsed()
                        .as_millis()
                        .min(u128::from(u64::MAX)) as u64,
                ),
            ),
        ]))
    }

    fn cloud_target_candidates(&self) -> Option<Vec<CloudTarget>> {
        let p = self.weather.published.as_ref()?;
        if self.render_environment_frame.context != Some(p.request.stamp.context)
            || self.application.core_view().manifest != Some(p.request.stamp.manifest)
        {
            return None;
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
        let mut candidates: Vec<_> = p
            .snapshot
            .clouds
            .iter()
            .filter_map(|cloud| {
                let age = utc - cloud.birth_utc;
                if !(420.0..=2800.0).contains(&age) {
                    return None;
                }
                let center =
                    cloud.direction_at(utc) * (radius + cloud.base_m + cloud.size_m.y * 0.5);
                let delta = center - eye;
                let distance_m = delta.length();
                let direction = delta.normalize_or_zero();
                let pitch_deg = direction.dot(up).clamp(-1.0, 1.0).asin().to_degrees();
                if pitch_deg < 5.0 {
                    return None;
                }
                let yaw_deg = direction
                    .dot(right)
                    .atan2(direction.dot(forward))
                    .to_degrees();
                let radius_deg = (cloud.extent_m() / distance_m.max(1.0)).atan().to_degrees();
                Some(CloudTarget {
                    id: cloud.id,
                    kind: format!("{:?}", cloud.kind),
                    opacity: f64::from(cloud.opacity),
                    distance_m,
                    radius_deg,
                    yaw_deg,
                    pitch_deg,
                    score: f64::from(cloud.opacity) * radius_deg,
                })
            })
            .collect();
        candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
        candidates.truncate(8);
        Some(candidates)
    }
}
