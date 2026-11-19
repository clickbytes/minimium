use super::{DesktopHost, V};
use crate::renderer::render_queries::{finite, record};
use minimium_renderer::milky_way_gpu::MAX_BOLIDS;

impl DesktopHost {
    pub(super) fn bolid_status_report(&self) -> Option<V> {
        let context = self.render_environment_frame.context?;
        let matching = self
            .bolids
            .iter()
            .filter(|(scope, _, _)| *scope == context)
            .collect::<Vec<_>>();
        if matching.is_empty() {
            return None;
        }
        let rows = matching
            .iter()
            .take(MAX_BOLIDS)
            .map(|(_, event, model)| {
                let mut fields = vec![
                    ("id", V::U64(event.id)),
                    ("kind", V::U64(u64::from(event.kind))),
                    ("seed", V::U64(event.seed)),
                    ("duration_ms", V::U64(u64::from(event.duration_ms))),
                    ("start_simulation_ms", V::U64(event.start_simulation_ms)),
                    ("model_ready", V::Bool(model.is_some())),
                ];
                if let Some(model) = model {
                    let d = model.diagnostics();
                    fields.extend([
                        ("family", V::Enum(format!("{:?}", model.family()))),
                        ("initial_mass_kg", finite(d.initial_mass_kg)),
                        ("surviving_mass_kg", finite(d.surviving_mass_kg)),
                        ("deposited_mass_kg", finite(d.deposited_mass_kg)),
                        ("fractures", V::U64(u64::from(d.fractures))),
                        ("grain_releases", V::U64(u64::from(d.grain_releases))),
                        (
                            "peak_solid_emitters",
                            V::U64(u64::from(d.peak_solid_emitters)),
                        ),
                        ("luminous_end_s", finite(f64::from(d.luminous_end_s))),
                        ("peak_radiant_power_w", finite(d.peak_radiant_power_w)),
                    ]);
                }
                record(fields)
            })
            .collect::<Vec<_>>();
        let mut report = record([
            (
                "rows",
                V::Page {
                    rows,
                    next_cursor: None,
                },
            ),
            ("total", V::U64(matching.len() as u64)),
            ("limit", V::U64(MAX_BOLIDS as u64)),
            ("truncated", V::Bool(matching.len() > MAX_BOLIDS)),
            ("gpu_timing", V::Enum("not_instrumented".into())),
        ]);
        self.bolid_context(&mut report);
        Some(report)
    }

    pub(super) fn bolid_fragments_report(&self) -> Option<V> {
        self.render_environment_frame.context?;
        let frame = &self.prepared_bolids;
        let count = (frame.count[0] as usize).min(MAX_BOLIDS);
        let rows = frame
            .events
            .iter()
            .take(count)
            .enumerate()
            .map(|(event_index, event)| {
                let mut heads = std::iter::once(event.head_power)
                    .chain(event.fragments.iter().copied())
                    .enumerate()
                    .filter(|(_, head)| head[3] > 1e-6)
                    .collect::<Vec<_>>();
                heads.sort_by(|a, b| b.1[3].total_cmp(&a.1[3]));
                let emitting = heads.len();
                let fragments = heads
                    .into_iter()
                    .take(6)
                    .map(|(index, head)| {
                        let point = glam::Vec3::new(head[0], head[1], head[2]);
                        let tail = event.tails[index];
                        let tail = glam::Vec3::new(tail[0], tail[1], tail[2]);
                        record([
                            ("emitter_index", V::U64(index as u64)),
                            (
                                "kind",
                                V::Enum(
                                    if event.optics[index][3] >= 1.0 {
                                        "grains"
                                    } else {
                                        "solid"
                                    }
                                    .into(),
                                ),
                            ),
                            ("power", finite(f64::from(head[3]))),
                            ("wake_world_m", finite(f64::from(point.distance(tail)))),
                        ])
                    })
                    .collect::<Vec<_>>();
                record([
                    ("event_index", V::U64(event_index as u64)),
                    ("age_presentation_s", finite(f64::from(event.start_age[3]))),
                    ("emitting", V::U64(emitting as u64)),
                    (
                        "fragments",
                        V::Page {
                            rows: fragments,
                            next_cursor: None,
                        },
                    ),
                    ("fragments_truncated", V::Bool(emitting > 6)),
                ])
            })
            .collect::<Vec<_>>();
        let mut report = record([
            (
                "rows",
                V::Page {
                    rows,
                    next_cursor: None,
                },
            ),
            ("limit", V::U64(MAX_BOLIDS as u64)),
            ("truncated", V::Bool(frame.count[0] as usize > MAX_BOLIDS)),
        ]);
        self.bolid_context(&mut report);
        Some(report)
    }

    pub(super) fn bolid_lighting_report(&self) -> Option<V> {
        self.render_environment_frame.context?;
        let frame = &self.render_environment_frame;
        let gpu = &self.renderer;
        let base = frame.environment_uniform.ambient_rgb_exposure[3];
        let night_gain = minimium_renderer::render_frame::view_exposure(
            frame.render_view,
            frame.environment_uniform.global_sun_direction,
            1.0,
        );
        let mut fields = vec![
            ("hdr", V::Bool(gpu.hdr_enabled)),
            (
                "physical_atmosphere",
                V::Bool(gpu.hdr_enabled && gpu.atmosphere_gpu.is_active()),
            ),
            ("base_exposure", finite(f64::from(base))),
            ("night_gain", finite(f64::from(night_gain))),
            (
                "terrain_debug",
                V::U64(u64::from(gpu.terrain_gpu.uniform.debug[0])),
            ),
            ("gpu_timing", V::Enum("not_instrumented".into())),
        ];
        if let Some((position, color)) = super::super::bolids::strongest_bolid_light(
            &self.prepared_bolids,
            frame.render_view.camera_eye_world_m,
            gpu.atmosphere_gpu.optical_uniform(),
        ) {
            let incident = minimium_renderer::bolid_light::incident_luminance(
                position,
                color,
                frame.render_view.camera_eye_world_m,
                gpu.atmosphere_gpu.optical_uniform(),
            );
            fields.push(("incident_available", V::Bool(true)));
            fields.push(("incident_luminance", finite(incident)));
            fields.push((
                "exposed_incident",
                finite(incident * f64::from(base * night_gain)),
            ));
        } else {
            fields.push(("incident_available", V::Bool(false)));
        }
        let mut report = record(fields);
        self.bolid_context(&mut report);
        Some(report)
    }
}
