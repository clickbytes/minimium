use super::ClientApplication;
use crate::client_commands::{output, ClientPropertyCommand, CommandSource, FrontendAction};
use minimium_commands::admin::{AdminReplyV1, Correlation};
use minimium_commands::operation::CommandValue;
use minimium_commands::results::{
    OperationContext, OperationFailure, OperationReceipt, OperationResult, Payload, Stage,
};

impl ClientApplication {
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
