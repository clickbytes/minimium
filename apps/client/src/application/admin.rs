use super::ClientApplication;
use crate::client_commands::{output, ClientPropertyCommand, CommandSource, FrontendAction};
use minimium_commands::admin::{AdminReplyV1, Correlation};
use minimium_commands::operation::CommandValue;
use minimium_commands::results::{
    OperationContext, OperationFailure, OperationReceipt, OperationResult, Payload, Stage,
};

impl ClientApplication {
    pub(crate) fn admin_operation(
        &mut self,
        identity: &OperationContext,
        action: &FrontendAction,
        source: CommandSource,
    ) -> Option<output::ClientResult> {
        let before = self.connection_view().pending_admin();
        let now = crate::host_time::sample();
        let requested = match action {
            FrontendAction::SpawnRandomBolid(count) => self.request_random_bolid(*count, now),
            FrontendAction::SpawnRandomMeteorid => self.request_random_meteorid(now),
            FrontendAction::BolidStatus(id) => self.request_bolid_shower_status(*id, now),
            FrontendAction::SaveWorld => self.request_persistence_save(now),
            FrontendAction::SaveStatus(id) => self.request_persistence_status(*id, now),
            FrontendAction::ImpactStatus { world, id } => {
                self.request_impact_receipt_status(*world, *id, now)
            }
            _ => return None,
        };
        Some(match requested {
            Ok(()) => self
                .admin_submission(before, identity, source)
                .unwrap_or_else(|| {
                    output::native(OperationResult::new(
                        identity.clone(),
                        OperationReceipt::at(Stage::Rejected),
                        Err(OperationFailure {
                            code: "unavailable".into(),
                            subject: Some(identity.operation.0.clone()),
                            reason: "admin request had no pending correlation".into(),
                            usage: None,
                            span: None,
                            origin: "client_admin".into(),
                        }),
                    ))
                }),
            Err(error) => output::native(OperationResult::new(
                identity.clone(),
                OperationReceipt::at(Stage::Rejected),
                Err(OperationFailure {
                    code: error.code().into(),
                    subject: Some(identity.operation.0.clone()),
                    reason: error.to_string(),
                    usage: None,
                    span: None,
                    origin: "client_admin".into(),
                }),
            )),
        })
    }

    pub(crate) fn policy_operation(
        &mut self,
        identity: &OperationContext,
        action: &FrontendAction,
        source: CommandSource,
    ) -> Option<output::ClientResult> {
        let now = crate::host_time::sample();
        let result = match action {
            FrontendAction::Jump => self.core.request_jump(now),
            FrontendAction::Movement(_, Some(patch)) => {
                self.core.request_movement_policy(*patch, None, now)
            }
            FrontendAction::Movement(field, None) => {
                self.core
                    .connection_view()
                    .movement_property(*field, identity.request_id, now)
            }
            _ => return None,
        };
        if result.receipt.stage == Stage::Submitted {
            if let Some(session) = result.identity.session {
                self.policy_output_sources
                    .insert((session, result.identity.request_id), source);
            }
        }
        Some(output::native(result))
    }
    pub(crate) fn finish_policy(
        &mut self,
        result: OperationResult,
    ) -> (Option<CommandSource>, output::ClientResult) {
        #[cfg(feature = "console-scripting")]
        self.script_owner_result(&result);
        let source = result.identity.session.and_then(|session| {
            let key = (session, result.identity.request_id);
            if result.receipt.stage == Stage::OwnerAccepted {
                self.policy_output_sources.get(&key).copied()
            } else {
                self.policy_output_sources.remove(&key)
            }
        });
        (source, output::native(result))
    }

    pub(crate) fn clock_operation(
        &mut self,
        identity: OperationContext,
        action: &crate::client_commands::time_adapter::ClockAction,
        frame: crate::environment::EnvironmentFrameInput,
        source: CommandSource,
    ) -> output::ClientResult {
        use crate::client_commands::time_adapter::{self, ClockAction};
        use minimium_client_runtime::ControllerError;
        let now = crate::host_time::sample();
        let result = match action {
            ClockAction::Read(_) | ClockAction::Status => {
                if let Some(payload) = time_adapter::read(action, frame, &identity.operation.0) {
                    let mut receipt = OperationReceipt::at(Stage::Completed);
                    receipt.revision = frame
                        .committed
                        .map(|s| s.revision().get())
                        .or(frame.revision);
                    receipt.freshness =
                        Some(minimium_commands::results::property::Freshness::Sampled);
                    return output::native(OperationResult::new(identity, receipt, Ok(payload)));
                }
                Err(ControllerError::new(
                    "unavailable",
                    "clock sample unavailable",
                ))
            }
            ClockAction::InitialConfig => self.core.request_initial_clock_config(now),
            ClockAction::Write(change) => self.core.request_clock(*change, now),
            ClockAction::Scale(text) => minimium_environment::parse_scale_ppm(text)
                .map_err(|reason| ControllerError::new("invalid_value", reason))
                .and_then(|scale| {
                    self.core.request_clock(
                        minimium_environment::clock_command::ClockWrite::Scale(scale),
                        now,
                    )
                }),
        };
        let result = match result {
            Ok(c) => self.submitted(identity, source, c),
            Err(error) => {
                let subject = Some(identity.operation.0.clone());
                OperationResult::new(
                    identity,
                    OperationReceipt::at(Stage::Rejected),
                    Err(OperationFailure {
                        code: error.code().into(),
                        subject,
                        reason: error.to_string(),
                        usage: None,
                        span: None,
                        origin: "client_clock".into(),
                    }),
                )
            }
        };
        output::native(result)
    }

    pub(crate) fn admin_help(
        &mut self,
        identity: &OperationContext,
        action: &FrontendAction,
        source: CommandSource,
    ) -> Option<output::ClientResult> {
        let FrontendAction::Property(ClientPropertyCommand::Help(key)) = action else {
            return None;
        };
        let target = key.key();
        if !target.starts_with("world.") || !self.connection_view().admin_available() {
            return None;
        }
        let result = match self
            .core
            .request_admin_help(target, crate::host_time::sample())
        {
            Ok(c) => self.submitted(identity.clone(), source, c),
            Err(error) => OperationResult::new(
                identity.clone(),
                OperationReceipt::at(Stage::Rejected),
                Err(OperationFailure {
                    code: error.code().into(),
                    subject: Some(target.into()),
                    reason: error.to_string(),
                    usage: None,
                    span: None,
                    origin: "client_admin".into(),
                }),
            ),
        };
        Some(output::native(result))
    }

    fn submitted(
        &mut self,
        mut identity: OperationContext,
        source: CommandSource,
        c: Correlation,
    ) -> OperationResult {
        self.admin_output_source = Some((c, source));
        identity.request_id = c.request_id;
        identity.correlation_nonce = Some(c.nonce);
        OperationResult::new(
            identity,
            OperationReceipt::at(Stage::Submitted),
            Ok(Payload::Value(CommandValue::Unit)),
        )
    }

    pub(crate) fn admin_submission(
        &mut self,
        before: Option<Correlation>,
        identity: &OperationContext,
        source: CommandSource,
    ) -> Option<output::ClientResult> {
        let after = self.connection_view().pending_admin()?;
        (before != Some(after))
            .then(|| output::native(self.submitted(identity.clone(), source, after)))
    }

    pub(crate) fn finish_admin(
        &mut self,
        reply: AdminReplyV1,
    ) -> (Option<CommandSource>, output::ClientResult) {
        #[cfg(feature = "console-scripting")]
        self.script_owner_result(&reply.result);
        let correlation = Correlation {
            nonce: reply.nonce,
            request_id: reply.result.identity.request_id,
        };
        let source = if self
            .admin_output_source
            .as_ref()
            .is_some_and(|(c, _)| *c == correlation)
        {
            self.admin_output_source.take().map(|(_, source)| source)
        } else {
            None
        };
        (source, output::native(reply.result))
    }
}
