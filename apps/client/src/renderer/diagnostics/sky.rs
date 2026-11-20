use super::super::DesktopHost;

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
}
