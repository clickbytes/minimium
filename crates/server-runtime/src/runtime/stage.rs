use super::*;
impl ServerRuntime {
    pub(super) fn run_system_stage(&mut self, stage: crate::modules::Stage) -> io::Result<()> {
        if stage == crate::modules::Stage::Startup {
            let startup_worlds: Vec<_> = self
                .game
                .worlds
                .iter()
                .map(|(instance, owner)| {
                    let lifecycle = if owner.content_initialized {
                        crate::modules::WorldLifecycle::Restored
                    } else {
                        crate::modules::WorldLifecycle::New
                    };
                    (instance, lifecycle)
                })
                .collect();
            for (instance, lifecycle) in startup_worlds {
                let feedback_diagnostics = self.schedule.diagnostics();
                let batch = self.schedule.run_stage(
                    stage,
                    self.step_boundary().tick(),
                    &self.game.worlds,
                    self.prepared_geography.as_ref(),
                    &self.game.gameplay_events,
                    Some((instance, lifecycle)),
                );
                self.report_effect_feedback_diagnostics(feedback_diagnostics);
                let outcome = self.commit_effect_batch(batch)?;
                self.schedule.publish_effect_receipts(&outcome)?;
                self.report_effect_batch_outcome(&format!("{stage:?}"), &outcome);
                if outcome.has_rejections() {
                    let receipts = self.schedule.drain_pending_effect_receipts();
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        StartupEffectFailure::new(instance, receipts, outcome.rejected_pushes()),
                    ));
                }
            }
            self.game.worlds.initialize_new_content();
            return Ok(());
        }
        let feedback_diagnostics = self.schedule.diagnostics();
        let batch = self.schedule.run_stage(
            stage,
            self.step_boundary().tick(),
            &self.game.worlds,
            self.prepared_geography.as_ref(),
            &self.game.gameplay_events,
            None,
        );
        self.report_effect_feedback_diagnostics(feedback_diagnostics);
        let outcome = self.commit_effect_batch(batch)?;
        self.schedule.publish_effect_receipts(&outcome)?;
        self.report_effect_batch_outcome(&format!("{stage:?}"), &outcome);
        Ok(())
    }
}
