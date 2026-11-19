use super::runtime::DesktopHost;
use glam::DVec3;
use minimium_environment::weather;
impl DesktopHost {
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
        let timing = format!("frame_utc_ms={:?} cloud_interval_s={:?} ground_fog_interval_s={:?} cloud_detail={} cover_detail={} expired={}",
            self.environment_frame.render_world_utc_ms, self.weather.published.as_ref().map(|p| p.interval()),
            self.weather.cover.as_ref().map(|c| (c.start_utc,c.start_utc+3600.0)),
            self.weather.published.as_ref().map_or("unavailable", |p| if !p.coarse {"full"} else if p.stats.cached > 0 {"mixed"} else {"local"}),
            if self.weather.cover.is_none() {"unavailable"} else if self.weather.coarse_cover {"column"} else {"tile"}, self.weather.expired);
        let Some(p) = &self.weather.published else {
            return vec![
                timing,
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
        vec![timing,
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
