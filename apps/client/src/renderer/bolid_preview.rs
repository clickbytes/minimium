//! Local retained-event preview operations. No network or save write occurs here.
mod reports;

use super::DesktopHost;
use crate::client_commands::{
    bolid_preview::BolidPreviewOperation as Op, output as dispatch, PresentationCapability,
    PresentationRequirement,
};
use minimium_commands::operation::CommandValue as V;
use minimium_commands::results::property::{
    ConfigurationSource, Freshness, ReadSource, StartupValue,
};
use minimium_commands::results::{
    OperationContext, OperationReceipt, OperationResult, Payload, Stage,
};
use minimium_commands::{
    CommandError, CommandErrorCode as Code, PropertyLifecycle, PropertySource, PropertyValue,
};

impl DesktopHost {
    pub(in crate::renderer) fn bolid_preview_operation(
        &mut self,
        identity: &OperationContext,
        op: Op,
    ) -> dispatch::ClientResult {
        if !matches!(&op, Op::Transmission(_) | Op::TransmissionRead) {
            if let Some(unavailable) = super::controller::presentation::unavailable_for(
                self,
                PresentationCapability::DeveloperRenderer,
                PresentationRequirement::ActiveWorldPresentation,
            ) {
                return rejected(identity, Code::Unavailable, unavailable.message());
            }
        }
        let outcome = match op {
            Op::Transmission(enabled) => {
                if !self
                    .renderer
                    .milky_way
                    .set_transmission_diagnostic(&self.renderer.queue, enabled)
                {
                    return rejected(
                        identity,
                        Code::Unavailable,
                        "bolid layer unavailable".into(),
                    );
                }
                self.bolid_transmission_requested = Some(enabled);
                self.bolid_transmission_revision =
                    self.bolid_transmission_revision.saturating_add(1);
                (
                    Stage::OwnerCommitted,
                    Payload::Property(self.bolid_transmission_property()),
                )
            }
            Op::TransmissionRead => (
                Stage::Completed,
                Payload::Property(self.bolid_transmission_property()),
            ),
            Op::RestorePath(path) => match self.restore_preview_path(path) {
                Ok(value) => (Stage::OwnerCommitted, Payload::Value(value)),
                Err((code, reason)) => return rejected(identity, code, reason),
            },
            Op::Replay {
                age_seconds,
                seed,
                event_id,
                frozen,
            } => match self.replay_preview(age_seconds, seed, event_id, frozen) {
                Ok(value) => (Stage::OwnerCommitted, Payload::Value(value)),
                Err((code, reason)) => return rejected(identity, code, reason),
            },
            Op::Status => match self.bolid_status_report() {
                Some(value) => (Stage::Completed, Payload::Value(value)),
                None => {
                    return rejected(
                        identity,
                        Code::Unavailable,
                        "retained bolid sample unavailable".into(),
                    )
                }
            },
            Op::Fragments => match self.bolid_fragments_report() {
                Some(value) => (Stage::Completed, Payload::Value(value)),
                None => {
                    return rejected(
                        identity,
                        Code::Unavailable,
                        "prepared bolid fragment sample unavailable".into(),
                    )
                }
            },
            Op::Lighting => match self.bolid_lighting_report() {
                Some(value) => (Stage::Completed, Payload::Value(value)),
                None => {
                    return rejected(
                        identity,
                        Code::Unavailable,
                        "prepared bolid lighting sample unavailable".into(),
                    )
                }
            },
        };
        dispatch::native(OperationResult::new(
            identity.clone(),
            OperationReceipt::at(outcome.0),
            Ok(outcome.1),
        ))
    }

    fn bolid_transmission_property(&self) -> PropertyValue<V> {
        let requested = self.bolid_transmission_requested.unwrap_or(false);
        let effective = self.renderer.milky_way.transmission_diagnostic();
        let source = if self.bolid_transmission_requested.is_some() {
            PropertySource::Session
        } else {
            PropertySource::Default
        };
        PropertyValue {
            lifecycle: PropertyLifecycle::Live,
            requested: Some(V::Bool(requested)),
            effective: effective.map(V::Bool),
            default: Some(V::Bool(false)),
            startup: Some(StartupValue {
                value: V::Bool(false),
                source: ConfigurationSource::Default,
            }),
            session_override: self.bolid_transmission_requested.map(V::Bool),
            source,
            read_source: ReadSource::LocalOwner,
            revision: Some(self.bolid_transmission_revision),
            freshness: Freshness::Current,
            difference_reason: if effective.is_none() {
                Some("bolid layer unavailable".into())
            } else if effective != Some(requested) {
                Some("bolid layer was reset during resize".into())
            } else {
                None
            },
        }
    }

    fn restore_preview_path(
        &mut self,
        path: crate::client_commands::BolidPath,
    ) -> Result<V, (Code, String)> {
        let context = self
            .render_environment_frame
            .context
            .ok_or((Code::StaleContext, "no prepared world context".into()))?;
        let now = self
            .application
            .connection_view()
            .session()
            .and_then(|session| session.environment_frame().simulation_ms)
            .ok_or((
                Code::Unavailable,
                "synchronized server simulation time unavailable".into(),
            ))?;
        let candidate = minimium_protocol::BolidEvent {
            id: path.id,
            kind: path.kind,
            seed: path.seed,
            duration_ms: path.duration_ms,
            start_world_m: path.start_world_m,
            end_world_m: path.end_world_m,
            start_simulation_ms: now,
        };
        if !candidate.valid() {
            return Err((Code::InvalidValue, "invalid bolid path".into()));
        }
        let index = self
            .bolids
            .iter()
            .position(|(scope, event, _)| *scope == context && event.id == candidate.id)
            .ok_or_else(|| {
                if self
                    .bolids
                    .iter()
                    .any(|(_, event, _)| event.id == candidate.id)
                {
                    (
                        Code::StaleContext,
                        "retained event belongs to another world".into(),
                    )
                } else {
                    (Code::Unavailable, "no matching retained event".into())
                }
            })?;
        let (_, event, cached) = &mut self.bolids[index];
        *event = candidate;
        *cached = None;
        self.diagnostic_bolid_age = Some((context, candidate.id, 0.0));
        let mut receipt = super::render_queries::record([
            ("id", V::U64(candidate.id)),
            ("kind", V::U64(u64::from(candidate.kind))),
            ("seed", V::U64(candidate.seed)),
            ("duration_ms", V::U64(u64::from(candidate.duration_ms))),
            ("frozen_age_s", super::render_queries::finite(0.0)),
            ("scope", V::Enum("local_preview".into())),
        ]);
        self.bolid_context(&mut receipt);
        Ok(receipt)
    }

    fn replay_preview(
        &mut self,
        age: f32,
        seed: Option<u64>,
        event_id: Option<u64>,
        frozen: bool,
    ) -> Result<V, (Code, String)> {
        if !age.is_finite() || !(0.0..=120.0).contains(&age) {
            return Err((
                Code::InvalidValue,
                "age must be 0..=120 presentation seconds".into(),
            ));
        }
        let context = self
            .render_environment_frame
            .context
            .ok_or((Code::StaleContext, "no prepared world context".into()))?;
        let now = self
            .application
            .connection_view()
            .session()
            .and_then(|session| session.environment_frame().simulation_ms)
            .ok_or((
                Code::Unavailable,
                "synchronized server simulation time unavailable".into(),
            ))?;
        let start = now.checked_sub((f64::from(age) * 1000.0) as u64).ok_or((
            Code::InvalidValue,
            "simulation time is too early for requested age".into(),
        ))?;
        let index = self
            .bolids
            .iter()
            .rposition(|(scope, event, _)| {
                *scope == context && event_id.is_none_or(|id| id == event.id)
            })
            .ok_or_else(|| {
                if event_id.is_some_and(|id| self.bolids.iter().any(|(_, event, _)| event.id == id))
                {
                    (
                        Code::StaleContext,
                        "retained event belongs to another world".into(),
                    )
                } else {
                    (Code::Unavailable, "no matching retained event".into())
                }
            })?;
        let (_, event, cached) = &mut self.bolids[index];
        event.start_simulation_ms = start;
        if let Some(seed) = seed {
            event.seed = seed;
            *cached = None;
        }
        let id = event.id;
        let effective_seed = event.seed;
        self.diagnostic_bolid_age = frozen.then_some((context, id, age));
        let mut receipt = super::render_queries::record([
            ("id", V::U64(id)),
            ("seed", V::U64(effective_seed)),
            (
                "age_presentation_s",
                super::render_queries::finite(f64::from(age)),
            ),
            ("frozen", V::Bool(frozen)),
            ("scope", V::Enum("local_preview".into())),
        ]);
        self.bolid_context(&mut receipt);
        Ok(receipt)
    }

    fn bolid_context(&self, value: &mut V) {
        let V::Record(fields) = value else { return };
        fields.insert("source".into(), V::Enum("local_bolid_preview".into()));
        fields.insert("freshness".into(), V::Enum("prepared_presentation".into()));
        if let Some(context) = self.render_environment_frame.context {
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
        if let Some(revision) = self.render_environment_frame.clock_revision {
            fields.insert("clock_revision".into(), V::U64(revision.get()));
        }
    }
}

fn rejected(identity: &OperationContext, code: Code, reason: String) -> dispatch::ClientResult {
    dispatch::failure(
        identity.clone(),
        Stage::Rejected,
        "LocalBolidPreview",
        CommandError::new(
            code,
            Some(identity.operation.0.as_str()),
            reason,
            None::<String>,
        ),
    )
}
