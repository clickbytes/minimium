use super::*;

impl ServerRuntime {
    pub(super) fn execute_module_command(
        &mut self,
        identity: Option<&str>,
        argv: &[String],
    ) -> commands::CommandReply {
        let mut effects = crate::modules::WorldEffectBuffer::default();
        let mut reply = commands::execute_module_command(
            identity,
            argv,
            &self.game.worlds,
            &mut effects,
            &self.module_registry,
        );
        if reply.handler_succeeded() {
            let batch = effects.into_batch();
            if !batch.effects().is_empty() || batch.rejected_pushes() > 0 {
                match self.commit_effect_batch(batch) {
                    Ok(outcome) => {
                        self.report_effect_batch_outcome("command", &outcome);
                        reply.set_effect_outcome(outcome);
                    }
                    Err(error) => {
                        reply = commands::CommandReply::error(
                            "server.command.effect_commit_failed",
                            &format!("could not commit command effects: {error}"),
                        );
                    }
                }
            }
        }
        reply
    }
}
