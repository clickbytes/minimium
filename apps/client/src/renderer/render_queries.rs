use super::DesktopHost;
use crate::client_commands::{
    render_queries::RenderQuery, PresentationOperation, PresentationRequirement,
};
use minimium_commands::operation::{CommandValue as V, FiniteValue};
use minimium_commands::results::{
    OperationContext, OperationReceipt, OperationResult, Payload, Stage,
};
use minimium_commands::{CommandError, CommandErrorCode};
use std::collections::BTreeMap;

pub(super) fn record(fields: impl IntoIterator<Item = (&'static str, V)>) -> V {
    V::Record(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

pub(super) fn finite(value: f64) -> V {
    V::Finite(FiniteValue::new(value).expect("finite prepared presentation value"))
}

impl DesktopHost {
    pub(in crate::renderer) fn render_query_operation(
        &self,
        identity: &OperationContext,
        query: RenderQuery,
    ) -> OperationResult {
        let unavailable = if matches!(query, RenderQuery::Frames | RenderQuery::Water) {
            None
        } else {
            let operation = match query {
                RenderQuery::Sky => PresentationOperation::SkyInfo,
                RenderQuery::Bodies => PresentationOperation::SkyBodies,
                RenderQuery::Terrain => PresentationOperation::TerrainStatus,
                RenderQuery::Water => PresentationOperation::WaterStatus,
                RenderQuery::Atmosphere => PresentationOperation::AtmosphereStatus,
                RenderQuery::Weather | RenderQuery::CloudTargets => {
                    PresentationOperation::WeatherStatus
                }
                RenderQuery::Frames => unreachable!(),
            };
            super::controller::presentation::unavailable_for(
                self,
                operation.capability(),
                PresentationRequirement::ReadyPresentation,
            )
            .map(|reason| reason.message())
        };
        let value = unavailable.as_ref().map_or_else(
            || match query {
                RenderQuery::Frames => self.metrics.frame_stats.report(),
                RenderQuery::Sky => self.sky_report(),
                RenderQuery::Bodies => self.bodies_report(),
                RenderQuery::Terrain => self.terrain_report(),
                RenderQuery::Water => Some(self.water_render_report()),
                RenderQuery::Atmosphere => self.atmosphere_report(),
                RenderQuery::Weather => self.weather_report(),
                RenderQuery::CloudTargets => self.cloud_targets_report(),
            },
            |_| None,
        );
        let Some(mut value) = value else {
            return OperationResult::new(
                identity.clone(),
                OperationReceipt::at(Stage::Rejected),
                Err(minimium_commands::results::OperationFailure::command(
                    "ClientHost",
                    CommandError::new(
                        CommandErrorCode::Unavailable,
                        Some(identity.operation.0.as_str()),
                        unavailable
                            .unwrap_or_else(|| "prepared presentation sample unavailable".into()),
                        None::<String>,
                    ),
                )),
            );
        };
        if query != RenderQuery::Water {
            if let V::Record(ref mut fields) = value {
                self.report_context(fields, query);
            }
        }
        OperationResult::new(
            identity.clone(),
            OperationReceipt::at(Stage::Completed),
            Ok(Payload::Value(value)),
        )
    }

    fn report_context(&self, fields: &mut BTreeMap<String, V>, query: RenderQuery) {
        let frame = &self.render_environment_frame;
        let freshness = match query {
            RenderQuery::Frames => "sampled_frame_history",
            RenderQuery::Sky | RenderQuery::Bodies => "prepared_current",
            RenderQuery::Terrain => "matching_geometry_publication",
            RenderQuery::Water => "owner_capture_with_stage_keys",
            RenderQuery::Atmosphere => "prepared_scene_async_timing",
            RenderQuery::Weather | RenderQuery::CloudTargets => "matching_weather_publication",
        };
        fields.insert("freshness".into(), V::Enum(freshness.into()));
        fields.insert("source".into(), V::Enum("local_renderer".into()));
        if let Some(context) = frame.context {
            fields.insert("world_instance".into(), V::U128(context.instance().get()));
            fields.insert(
                "content_revision".into(),
                V::U64(context.content_revision().get()),
            );
            fields.insert(
                "membership_epoch".into(),
                V::U64(context.membership_epoch().get()),
            );
        }
        if let Some(clock) = frame.clock_revision {
            fields.insert("clock_revision".into(), V::U64(clock.get()));
        }
        if let Some(utc) = self.environment_frame.render_world_utc_ms {
            fields.insert("world_utc_ms".into(), V::InstantMs(utc));
        }
    }

    fn atmosphere_report(&self) -> Option<V> {
        let gpu = &self.renderer.atmosphere_gpu;
        let mut fields = BTreeMap::new();
        fields.insert("resource_bytes".into(), V::U64(gpu.bytes()));
        fields.insert("debug_mode".into(), V::U64(u64::from(gpu.debug)));
        fields.insert("active".into(), V::Bool(gpu.is_active()));
        fields.insert("physical".into(), V::Bool(gpu.resources.physical));
        fields.insert(
            "observer_height_m".into(),
            finite(f64::from(gpu.optical_uniform().up_height[3])),
        );
        fields.insert(
            "planet_radius_km".into(),
            finite(f64::from(gpu.optical_uniform().radii[0]) / 1000.0),
        );
        fields.insert(
            "gpu_timing_scope".into(),
            V::Enum("sky_lut_aerial_lut_when_updated_plus_composite".into()),
        );
        if let Some((median, p95, samples, age_ms)) = gpu.timing_report() {
            fields.insert("gpu_timing".into(), V::Enum("measured".into()));
            fields.insert("gpu_median_ms".into(), finite(median));
            fields.insert("gpu_p95_ms".into(), finite(p95));
            fields.insert("gpu_samples".into(), V::U64(samples as u64));
            fields.insert("gpu_sample_age_ms".into(), V::U64(age_ms));
        } else {
            fields.insert(
                "gpu_timing".into(),
                V::Enum(
                    if gpu.timing_supported() {
                        "pending"
                    } else {
                        "unsupported"
                    }
                    .into(),
                ),
            );
        }
        Some(V::Record(fields))
    }
}
