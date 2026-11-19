use super::DesktopHost;
use crate::client_commands::{
    meteor_adapter::{MeteorDiagnosticReport, MeteorOperation},
    output as dispatch, MeteorDebug, MeteorLightDebug, PresentationCapability,
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
    CommandError, CommandErrorCode, PropertyLifecycle, PropertySource, PropertyValue,
};

impl DesktopHost {
    pub(in crate::renderer) fn meteor_operation(
        &mut self,
        identity: &OperationContext,
        operation: MeteorOperation,
    ) -> Option<dispatch::ClientResult> {
        let needs_frame = matches!(
            operation,
            MeteorOperation::Status
                | MeteorOperation::Spawn(..)
                | MeteorOperation::Debug(
                    MeteorDebug::Paths | MeteorDebug::Budget | MeteorDebug::Light
                )
        );
        if needs_frame {
            if let Some(unavailable) = super::controller::presentation::unavailable_for(
                self,
                PresentationCapability::MeteorDiagnostics,
                PresentationRequirement::ReadyPresentation,
            ) {
                return Some(rejected(
                    identity,
                    CommandErrorCode::Unavailable,
                    unavailable.message(),
                ));
            }
        }
        let outcome = match operation {
            MeteorOperation::Status => {
                let Some(mut report) = super::meteor_diagnostics::report(self) else {
                    return Some(rejected(
                        identity,
                        CommandErrorCode::Unavailable,
                        "prepared meteor sample unavailable".into(),
                    ));
                };
                self.meteor_context(&mut report);
                (Stage::Completed, report)
            }
            MeteorOperation::Spawn(preset, seed) => {
                let receipt = match self.meteors.spawn(preset, seed) {
                    Ok(receipt) => receipt,
                    Err(reason) => {
                        return Some(rejected(identity, CommandErrorCode::Unavailable, reason))
                    }
                };
                let mut report = super::render_queries::record([
                    ("id", V::U64(receipt.id)),
                    ("preset", V::Enum(receipt.preset.label().into())),
                    ("seed", V::U64(receipt.seed)),
                    (
                        "presentation_time_s",
                        super::render_queries::finite(receipt.presentation_time_s),
                    ),
                    ("scope", V::Enum("client_local".into())),
                ]);
                self.meteor_context(&mut report);
                (Stage::OwnerCommitted, report)
            }
            MeteorOperation::Clear => (Stage::OwnerCommitted, V::U64(self.meteors.clear() as u64)),
            MeteorOperation::Debug(mode) => {
                let snapshot = if matches!(
                    mode,
                    MeteorDebug::Paths | MeteorDebug::Budget | MeteorDebug::Light
                ) {
                    let Some(status) = super::meteor_diagnostics::report(self) else {
                        return Some(rejected(
                            identity,
                            CommandErrorCode::Unavailable,
                            "prepared meteor sample unavailable".into(),
                        ));
                    };
                    Some(if mode == MeteorDebug::Paths {
                        super::render_queries::record([
                            ("status", status),
                            ("paths", super::meteor_diagnostics::paths_report(self)),
                        ])
                    } else {
                        status
                    })
                } else {
                    None
                };
                self.meteors.set_debug_mode(mode);
                self.apply_meteor_debug_flags();
                return Some(dispatch::meteor_diagnostic(
                    identity.clone(),
                    Stage::OwnerCommitted,
                    MeteorDiagnosticReport {
                        property: self.meteor_debug_property(false),
                        snapshot,
                    },
                ));
            }
            MeteorOperation::Light(mode) => {
                self.meteors.set_light_mode(mode);
                self.apply_meteor_debug_flags();
                return Some(dispatch::meteor_diagnostic(
                    identity.clone(),
                    Stage::OwnerCommitted,
                    MeteorDiagnosticReport {
                        property: self.meteor_debug_property(true),
                        snapshot: None,
                    },
                ));
            }
            MeteorOperation::DebugRead => {
                return Some(dispatch::meteor_diagnostic(
                    identity.clone(),
                    Stage::Completed,
                    MeteorDiagnosticReport {
                        property: self.meteor_debug_property(false),
                        snapshot: None,
                    },
                ))
            }
            MeteorOperation::LightRead => {
                return Some(dispatch::meteor_diagnostic(
                    identity.clone(),
                    Stage::Completed,
                    MeteorDiagnosticReport {
                        property: self.meteor_debug_property(true),
                        snapshot: None,
                    },
                ))
            }
        };
        Some(dispatch::native(OperationResult::new(
            identity.clone(),
            OperationReceipt::at(outcome.0),
            Ok(Payload::Value(outcome.1)),
        )))
    }

    fn apply_meteor_debug_flags(&mut self) {
        self.renderer.transient_enabled = self.meteors.light_mode != MeteorLightDebug::Off;
        self.renderer.transient_transmission =
            self.meteors.light_mode == MeteorLightDebug::Transmission;
    }

    fn meteor_debug_property(&self, light: bool) -> PropertyValue<V> {
        let (current, default) = if light {
            (
                self.meteors.light_mode.label(),
                MeteorLightDebug::On.label(),
            )
        } else {
            (self.meteors.debug_mode.label(), MeteorDebug::Off.label())
        };
        let value = V::Enum(current.into());
        let source = if self.meteors.debug_revision > 0 {
            PropertySource::Session
        } else {
            PropertySource::Default
        };
        PropertyValue {
            lifecycle: PropertyLifecycle::Live,
            requested: Some(value.clone()),
            effective: Some(value.clone()),
            default: Some(V::Enum(default.into())),
            startup: Some(StartupValue {
                value: V::Enum(default.into()),
                source: ConfigurationSource::Default,
            }),
            session_override: (source == PropertySource::Session).then_some(value),
            source,
            read_source: ReadSource::LocalOwner,
            revision: Some(self.meteors.debug_revision),
            freshness: Freshness::Current,
            difference_reason: None,
        }
    }

    fn meteor_context(&self, report: &mut V) {
        let V::Record(fields) = report else { return };
        fields.insert("source".into(), V::Enum("local_meteor_runtime".into()));
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

fn rejected(
    identity: &OperationContext,
    code: CommandErrorCode,
    reason: String,
) -> dispatch::ClientResult {
    dispatch::failure(
        identity.clone(),
        Stage::Rejected,
        "LocalMeteorRuntime",
        CommandError::new(
            code,
            Some(identity.operation.0.as_str()),
            reason,
            None::<String>,
        ),
    )
}
