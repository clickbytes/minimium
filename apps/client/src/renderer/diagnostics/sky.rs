use super::super::DesktopHost;
use super::{
    frames::push_counter_fields,
    weather::{push_gpu_fields, push_shadow_fields},
};
use crate::environment::{diagnostics::format_utc_millis, SkyDiagnosticsSnapshot};

impl DesktopHost {
    pub(in crate::renderer) fn sky_report(
        &self,
    ) -> Option<minimium_commands::operation::CommandValue> {
        use super::super::render_queries::{finite, record};
        use minimium_commands::operation::CommandValue as V;
        let frame = &self.render_environment_frame;
        let sky = frame.sky.as_ref()?;
        let observer = frame.observer.as_ref()?;
        let mut report = record([
            (
                "requested_quality",
                V::Enum(self.sky_quality.requested.label().into()),
            ),
            (
                "effective_quality",
                V::Enum(self.sky_quality.effective.label().into()),
            ),
            (
                "observer_lat_deg",
                finite(observer.geographic.latitude_rad().to_degrees()),
            ),
            (
                "observer_lon_deg",
                finite(observer.geographic.longitude_rad().to_degrees()),
            ),
            ("observer_height_m", finite(observer.geographic.height_m())),
            (
                "sun_alt_deg",
                finite(sky.sun.geometric_altitude_rad.to_degrees()),
            ),
            (
                "sun_apparent_alt_deg",
                finite(sky.sun.apparent_altitude_rad.to_degrees()),
            ),
            (
                "sun_azimuth_deg",
                finite(sky.sun.azimuth_rad.to_degrees().rem_euclid(360.0)),
            ),
            (
                "moon_alt_deg",
                finite(sky.moon.body.geometric_altitude_rad.to_degrees()),
            ),
            (
                "moon_apparent_alt_deg",
                finite(sky.moon.body.apparent_altitude_rad.to_degrees()),
            ),
            (
                "moon_azimuth_deg",
                finite(sky.moon.body.azimuth_rad.to_degrees().rem_euclid(360.0)),
            ),
            (
                "moon_illuminated_fraction",
                finite(sky.moon.illuminated_fraction),
            ),
        ]);
        if let V::Record(ref mut fields) = report {
            if let Some(reason) = self.sky_quality.fallback_reason {
                fields.insert("fallback_reason".into(), V::Text(reason.into()));
            }
            if let Some(descriptor) = frame.descriptor {
                fields.insert(
                    "astronomy_model_revision".into(),
                    V::U64(descriptor.astronomy_model_revision().get() as u64),
                );
                fields.insert(
                    "catalogue_id".into(),
                    V::U64(u64::from(descriptor.catalogue_id().get())),
                );
            }
        }
        Some(report)
    }

    pub(in crate::renderer) fn bodies_report(
        &self,
    ) -> Option<minimium_commands::operation::CommandValue> {
        use super::super::render_queries::{finite, record};
        use minimium_commands::operation::CommandValue as V;
        let frame = &self.render_environment_frame;
        frame.sky?;
        const RESULT_BODY_LIMIT: usize = 128;
        let max = minimium_environment::universe::MAX_CELESTIAL_BODIES + 2;
        let rows = frame
            .observed_bodies
            .iter()
            .take(max.min(RESULT_BODY_LIMIT))
            .map(|body| {
                let flux = if body.id == minimium_environment::universe::SKY_SUN_ID {
                    frame
                        .local_sun
                        .as_ref()
                        .map(|local| f64::from(local.effective_linear_rgb.iter().sum::<f32>()))
                } else if body.id == minimium_environment::universe::SKY_MOON_ID {
                    frame
                        .local_moon
                        .as_ref()
                        .map(|local| f64::from(local.effective_linear_rgb.iter().sum::<f32>()))
                } else {
                    Some(
                        body.emitted_linear_rgb
                            .iter()
                            .chain(body.reflected_linear_rgb.iter())
                            .chain(body.scattered_linear_rgb.iter())
                            .sum::<f64>(),
                    )
                };
                let mut fields = vec![
                    ("id", V::U64(body.id.get())),
                    ("kind", V::Enum(format!("{:?}", body.kind))),
                    ("visible", V::Bool(body.visible)),
                    ("distance_m", finite(body.distance_m)),
                    ("phase", finite(body.phase)),
                ];
                if let Some(flux) = flux {
                    fields.push(("flux", finite(flux)));
                }
                record(fields)
            })
            .collect::<Vec<_>>();
        Some(record([
            (
                "rows",
                V::Page {
                    rows,
                    next_cursor: None,
                },
            ),
            ("total", V::U64(frame.observed_bodies.len() as u64)),
            (
                "truncated",
                V::Bool(frame.observed_bodies.len() > max.min(RESULT_BODY_LIMIT)),
            ),
        ]))
    }

    pub(in crate::renderer) fn sky_info_lines(&self) -> Vec<String> {
        let frame = &self.render_environment_frame;
        let input = self.environment_frame;
        let mut snapshot = SkyDiagnosticsSnapshot::new();
        snapshot.push(
            "profile",
            frame
                .descriptor
                .map_or("n/a", |value| profile_label(value.profile())),
        );
        snapshot.push("requested_quality", self.sky_quality.requested.label());
        snapshot.push("effective_quality", self.sky_quality.effective.label());
        snapshot.push(
            "fallback_reason",
            self.sky_quality.fallback_reason.unwrap_or("none"),
        );
        snapshot.push_optional(
            "astronomy_model_revision",
            frame
                .descriptor
                .map(|value| value.astronomy_model_revision().get()),
        );
        snapshot.push_optional(
            "catalogue_id",
            frame.descriptor.map(|value| value.catalogue_id().get()),
        );
        snapshot.push(
            "catalogue_sha256",
            self.sky_catalogue
                .as_ref()
                .map(|catalogue| sha256_hex(catalogue.identity().sha256))
                .unwrap_or_else(|| "n/a".to_owned()),
        );
        snapshot.push_optional(
            "catalogue_count",
            self.sky_catalogue
                .as_ref()
                .map(|catalogue| catalogue.records().len()),
        );
        push_clock_fields(&mut snapshot, input);
        push_observer_fields(&mut snapshot, frame);
        push_body_fields(&mut snapshot, frame);
        push_counter_fields(
            &mut snapshot,
            self.metrics.frame_stats.environment_snapshot(),
        );
        push_shadow_fields(&mut snapshot, self);
        push_gpu_fields(&mut snapshot, self);
        push_milky_way_fields(&mut snapshot, self);
        super::super::meteor_diagnostics::push(&mut snapshot, self);
        snapshot.lines().collect()
    }
}

fn push_milky_way_fields(snapshot: &mut SkyDiagnosticsSnapshot, renderer: &DesktopHost) {
    let frame = &renderer.render_environment_frame;
    let overview =
        frame.render_view.kind == super::super::environment::EnvironmentViewKind::Overview;
    let u = &frame.environment_uniform;
    let (stars, visibility) = minimium_renderer::atmosphere_view::celestial_visibility(
        u,
        renderer.renderer.atmosphere_gpu.optical_uniform(),
    );
    snapshot.push("star_visibility_before_extinction_and_clouds", stars);
    let drawn = renderer.renderer.environment_draw_calls.sky > 0;
    snapshot.push(
        "milky_way_state",
        if !drawn {
            "sky_not_drawn"
        } else if visibility <= 0.0 {
            "daylight_suppressed"
        } else if overview {
            "orbital_background"
        } else {
            "night_background"
        },
    );
    snapshot.push(
        "milky_way_visibility_before_extinction_and_clouds",
        visibility,
    );
    snapshot.push(
        "milky_way_texture_bytes",
        minimium_renderer::MilkyWayGpu::BYTES,
    );
    snapshot.push("scene_linear_HDR", renderer.renderer.hdr_enabled);
}

fn push_clock_fields(
    snapshot: &mut SkyDiagnosticsSnapshot,
    input: crate::environment::EnvironmentFrameInput,
) {
    let clock = input.clock_diagnostics;
    snapshot.push(
        "clock_source",
        clock.map_or("n/a", |value| value.source.label()),
    );
    snapshot.push_optional(
        "clock_mode",
        clock.and_then(|value| value.mode).map(clock_mode_label),
    );
    snapshot.push_optional(
        "clock_id",
        clock
            .and_then(|value| value.clock_id)
            .map(|value| value.get()),
    );
    snapshot.push_optional("clock_revision", input.revision);
    snapshot.push(
        "world_utc",
        input
            .render_world_utc_ms
            .map(format_utc_millis)
            .unwrap_or_else(|| "n/a".to_owned()),
    );
    snapshot.push_optional("rate_ppm", input.rate_ppm);
    snapshot.push_optional(
        "render_minus_estimated_utc_ms",
        input
            .render_world_utc_ms
            .zip(input.estimated_world_utc_ms)
            .map(|(render, estimate)| render.saturating_sub(estimate)),
    );
    snapshot.push(
        "rate",
        input.rate_ppm.map_or_else(
            || "n/a".to_owned(),
            |ppm| format!("{:.6}x", ppm as f64 / 1_000_000.0),
        ),
    );
    snapshot.push_optional(
        "offline_policy",
        clock
            .and_then(|value| value.offline_policy)
            .map(offline_policy_label),
    );
    snapshot.push_optional(
        "sample_age_real_ms",
        clock.and_then(|value| value.sample_age_real_ms),
    );
    snapshot.push(
        "clock_offset_ms",
        clock
            .and_then(|value| value.clock_offset_ms)
            .map_or_else(|| "n/a".to_owned(), |value| format!("{value:.3}")),
    );
    snapshot.push_optional("probe_rtt_ms", clock.and_then(|value| value.probe_rtt_ms));
    snapshot.push("clock_sync_state", input.quality.label());
    snapshot.push_optional(
        "clock_samples_accepted",
        clock.and_then(|value| value.samples_accepted),
    );
    snapshot.push_optional(
        "clock_samples_rejected",
        clock.and_then(|value| value.samples_rejected),
    );
}

fn push_observer_fields(
    snapshot: &mut SkyDiagnosticsSnapshot,
    frame: &super::super::environment::EnvironmentFrame,
) {
    let observer = frame.observer;
    snapshot.push(
        "observer_lat_deg",
        observer.map_or_else(
            || "n/a".to_owned(),
            |value| format!("{:.6}", value.geographic.latitude_rad().to_degrees()),
        ),
    );
    snapshot.push(
        "observer_lon_deg",
        observer.map_or_else(
            || "n/a".to_owned(),
            |value| format!("{:.6}", value.geographic.longitude_rad().to_degrees()),
        ),
    );
    snapshot.push(
        "observer_height_m",
        observer.map_or_else(
            || "n/a".to_owned(),
            |value| format!("{:.2}", value.geographic.height_m()),
        ),
    );
    snapshot.push(
        "observer_height_policy",
        observer.map_or("n/a", |value| match value.model {
            minimium_environment::sky::SkyModel::EarthGeoreferencedV1 => "EarthHeightReferenceV1",
            minimium_environment::sky::SkyModel::MappedEarthSkyV1 => "MappedGameHeightV1",
            minimium_environment::sky::SkyModel::None => "n/a",
        }),
    );
}

fn push_body_fields(
    snapshot: &mut SkyDiagnosticsSnapshot,
    frame: &super::super::environment::EnvironmentFrame,
) {
    let sky = frame.sky;
    snapshot.push_optional(
        "sun_alt_deg",
        sky.map(|value| value.sun.geometric_altitude_rad.to_degrees()),
    );
    snapshot.push_optional(
        "sun_apparent_alt_deg",
        sky.map(|value| value.sun.apparent_altitude_rad.to_degrees()),
    );
    snapshot.push_optional(
        "sun_azimuth_deg",
        sky.map(|value| value.sun.azimuth_rad.to_degrees().rem_euclid(360.0)),
    );
    snapshot.push_optional(
        "moon_alt_deg",
        sky.map(|value| value.moon.body.geometric_altitude_rad.to_degrees()),
    );
    snapshot.push_optional(
        "moon_apparent_alt_deg",
        sky.map(|value| value.moon.body.apparent_altitude_rad.to_degrees()),
    );
    snapshot.push_optional(
        "moon_azimuth_deg",
        sky.map(|value| value.moon.body.azimuth_rad.to_degrees().rem_euclid(360.0)),
    );
    snapshot.push_optional(
        "moon_illuminated_fraction",
        sky.map(|value| value.moon.illuminated_fraction),
    );
    snapshot.push_optional(
        "moon_angular_radius_rad",
        sky.map(|value| value.moon.body.angular_radius_rad),
    );
}

fn profile_label(profile: minimium_environment::EnvironmentProfile) -> &'static str {
    match profile {
        minimium_environment::EnvironmentProfile::Interior => "Interior",
        minimium_environment::EnvironmentProfile::Earth => "Earth",
        minimium_environment::EnvironmentProfile::EarthLike => "EarthLike",
    }
}

fn clock_mode_label(mode: minimium_environment::time::ClockMode) -> &'static str {
    match mode {
        minimium_environment::time::ClockMode::RealTime => "real_time",
        minimium_environment::time::ClockMode::TimeScale => "time_scale",
    }
}

fn offline_policy_label(policy: minimium_environment::time::OfflinePolicy) -> &'static str {
    match policy {
        minimium_environment::time::OfflinePolicy::Pause => "pause",
        minimium_environment::time::OfflinePolicy::Advance => "advance",
    }
}

fn sha256_hex(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
