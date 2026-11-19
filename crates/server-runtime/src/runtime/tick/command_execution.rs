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
            self.schedule.command_sequences_mut(),
        );
        if reply.handler_succeeded() {
            let batch = effects.into_batch();
            if !batch.effects().is_empty() || batch.rejected_pushes() > 0 {
                if self.environment_clock.rate().get() == 0 {
                    return commands::CommandReply::error("server.paused", "simulation paused");
                }
                if self.simulation_work_pending() {
                    return commands::CommandReply::error(
                        "server.busy",
                        "simulation boundary pending",
                    );
                }
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
        } else {
            let batch = effects.into_batch();
            if !batch.effects().is_empty() || batch.rejected_pushes() > 0 {
                let rejected =
                    vec![crate::modules::WorldEffectOutcome::StageRejected; batch.effects().len()];
                match batch.into_outcome(rejected) {
                    Ok(outcome) => {
                        self.report_effect_batch_outcome("command:handler_error", &outcome);
                        reply.set_effect_outcome(outcome);
                    }
                    Err(error) => super::super::diagnostics::emit_system_effect_diagnostic(
                        &self.world_logger,
                        "command:handler_error:receipt_invariant",
                        &format!("could not retain rejected effect receipts: {error:?}"),
                    ),
                }
            }
        }
        reply
    }
}
